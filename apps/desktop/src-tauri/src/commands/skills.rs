//! Skills commands: scan, import, bind, conflicts, git import.

use chm_core::domain::harness::{BindingType, HarnessSkillBinding};
use chm_core::domain::skills::Skill;
use chm_database::repos::harness::list_installations;
use chm_database::repos::skills::{create_skill, list_skill_bindings, list_skills};
use serde::Serialize;
use sqlx::{Pool, Sqlite};
use std::collections::HashMap;
use tauri::State;
use uuid::Uuid;

use crate::AppState;
use crate::skill_lib::scan_skill_dirs;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScannedSkillView {
    pub name: String,
    pub path: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSkillReport {
    pub imported: usize,
    pub duplicates: Vec<String>,
    pub conflicts: Vec<String>,
}

fn scan_dirs(pool: &Pool<Sqlite>, dir: &std::path::Path) -> Result<Vec<ScannedSkillView>, String> {
    let _ = pool;
    scan_skill_dirs(dir).map(|skills| {
        skills
            .into_iter()
            .map(|s| ScannedSkillView {
                name: s.name,
                path: s.path,
                content_hash: s.content_hash,
            })
            .collect()
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillView {
    pub id: String,
    pub name: String,
    pub canonical_path: String,
    pub content_hash: Option<String>,
    pub source_type: String,
    pub enabled: bool,
}

#[tauri::command]
pub async fn list_skills_cmd(state: State<'_, AppState>) -> Result<Vec<SkillView>, String> {
    let skills = list_skills(&state.pool).await.map_err(|e| e.to_string())?;
    Ok(skills
        .into_iter()
        .map(|s| SkillView {
            id: s.id.to_string(),
            name: s.name,
            canonical_path: s.canonical_path,
            content_hash: s.content_hash,
            source_type: s.source_type.as_str().to_string(),
            enabled: s.enabled,
        })
        .collect())
}

#[tauri::command]
pub async fn scan_skills_dir_cmd(
    state: State<'_, AppState>,
    dir: String,
) -> Result<Vec<ScannedSkillView>, String> {
    scan_dirs(&state.pool, std::path::Path::new(&dir))
}

pub async fn import_skills_core(
    pool: &Pool<Sqlite>,
    paths: &[String],
) -> Result<ImportSkillReport, String> {
    let mut report = ImportSkillReport::default();
    let existing = list_skills(pool).await.map_err(|e| e.to_string())?;
    // `existing` is a snapshot taken before the batch. Track successful
    // imports as we go so duplicate paths/content in one request receive the
    // same non-fatal treatment as duplicates already in the registry.
    let mut batch_paths = HashMap::<String, String>::new();
    let mut batch_hashes = std::collections::HashSet::<String>::new();
    for path in paths {
        let hash = crate::skill_lib::hash_directory(std::path::Path::new(path))?;
        let name = std::path::Path::new(path)
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .ok_or("invalid skill path")?;
        if existing
            .iter()
            .any(|sk| sk.content_hash.as_deref() == Some(hash.as_str()))
            || batch_hashes.contains(&hash)
        {
            report.duplicates.push(format!("skill:{name}"));
            continue;
        }
        if existing
            .iter()
            .any(|sk| sk.name == name && sk.canonical_path != *path)
            || batch_paths
                .get(&name)
                .is_some_and(|previous_path| previous_path != path)
        {
            // same name, different content at a different location = conflict
            report.conflicts.push(format!("skill:{name}"));
            continue;
        }
        if existing.iter().any(|sk| sk.canonical_path == *path) {
            report.duplicates.push(format!("skill:{name}"));
            continue;
        }
        if batch_paths.contains_key(&name) {
            report.duplicates.push(format!("skill:{name}"));
            continue;
        }
        let source_dir = std::path::Path::new(path)
            .parent()
            .and_then(|p| p.file_name())
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_else(|| "folder".into());
        let skill = Skill {
            id: Uuid::new_v4(),
            name: name.clone(),
            canonical_path: path.clone(),
            source_type: chm_core::domain::skills::SkillSourceType::Folder,
            source_url: None,
            content_hash: Some(hash.clone()),
            provenance: serde_json::json!({
                "source": source_dir,
                "imported_at": chrono::Utc::now().to_rfc3339(),
            }),
            enabled: true,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        create_skill(pool, &skill)
            .await
            .map_err(|e| e.to_string())?;
        batch_paths.insert(name, path.clone());
        batch_hashes.insert(hash);
        report.imported += 1;
    }
    Ok(report)
}

#[tauri::command]
pub async fn import_skills_cmd(
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> Result<ImportSkillReport, String> {
    import_skills_core(&state.pool, &paths).await
}

/// One-time adopt of ~/.agents/skills into the registry (idempotent by hash).
#[tauri::command]
pub async fn adopt_canonical_dir(state: State<'_, AppState>) -> Result<usize, String> {
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_default();
    let agents_dir = home.join(".agents/skills");
    if !agents_dir.is_dir() {
        return Ok(0);
    }
    let scanned = scan_skill_dirs(&agents_dir)?;
    let paths: Vec<String> = scanned.into_iter().map(|s| s.path).collect();
    let report = import_skills_core(&state.pool, &paths).await?;
    Ok(report.imported)
}

// --- bindings ---

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BindOutcome {
    pub binding_type: String,
    pub target_path: String,
}

async fn bind_skill_core(
    pool: &Pool<Sqlite>,
    installation_id: &str,
    skill_id: &str,
    home: &std::path::Path,
) -> Result<BindOutcome, String> {
    let sid = Uuid::parse_str(skill_id).map_err(|e| e.to_string())?;
    let inst = crate::commands::find_installation(pool, installation_id).await?;
    let iid = inst.id;
    let skill = list_skills(pool)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|s| s.id == sid)
        .ok_or("skill not found")?;

    let def = chm_harness_sdk::definition::all_definitions()
        .into_iter()
        .find(|d| d.id == inst.harness_type.as_str())
        .ok_or("no definition")?;

    // target dir from the definition's first skill path
    let target_dir = match def.skill_paths.first() {
        Some(rel) => home.join(rel),
        None => {
            return Ok(BindOutcome {
                binding_type: "unsupported".into(),
                target_path: String::new(),
            });
        }
    };
    // capability gate — only claude-code/opencode verified symlink-following (Phase 0)
    let supports_links = matches!(
        inst.harness_type,
        chm_core::domain::harness::HarnessType::ClaudeCode
            | chm_core::domain::harness::HarnessType::OpenCode
            | chm_core::domain::harness::HarnessType::Pi
            | chm_core::domain::harness::HarnessType::Reasonix
    );
    if !supports_links {
        return Ok(BindOutcome {
            binding_type: "unsupported".into(),
            target_path: target_dir.join(&skill.name).display().to_string(),
        });
    }

    let target = target_dir.join(&skill.name);
    let outcome =
        chm_filesystem::link_directory(std::path::Path::new(&skill.canonical_path), &target)
            .map_err(|e| e.to_string())?;
    let binding_type = match outcome {
        chm_filesystem::LinkOutcome::Symlink => BindingType::Symlink,
        chm_filesystem::LinkOutcome::Junction => BindingType::Junction,
        chm_filesystem::LinkOutcome::Copy => BindingType::Copy,
        chm_filesystem::LinkOutcome::AlreadyLinked => {
            // already linked: just record the binding row
            BindingType::Symlink
        }
        chm_filesystem::LinkOutcome::Unsupported(reason) => {
            return Ok(BindOutcome {
                binding_type: "unsupported".into(),
                target_path: reason,
            });
        }
    };
    if !matches!(outcome, chm_filesystem::LinkOutcome::AlreadyLinked)
        || list_skill_bindings(pool, iid)
            .await
            .map_err(|e| e.to_string())?
            .iter()
            .all(|b| b.target_path != target.display().to_string())
    {
        chm_database::repos::skills::create_skill_binding(
            pool,
            &HarnessSkillBinding {
                id: Uuid::new_v4(),
                harness_installation_id: iid,
                skill_id: sid,
                target_path: target.display().to_string(),
                binding_type,
                managed: true,
                status: "active".into(),
            },
        )
        .await
        .map_err(|e| e.to_string())?;
    }
    Ok(BindOutcome {
        binding_type: binding_type.as_str().to_string(),
        target_path: target.display().to_string(),
    })
}

#[tauri::command]
pub async fn bind_skill_cmd(
    state: State<'_, AppState>,
    installation_id: String,
    skill_id: String,
) -> Result<BindOutcome, String> {
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_default();
    bind_skill_core(&state.pool, &installation_id, &skill_id, &home).await
}

#[tauri::command]
pub async fn unbind_skill_cmd(
    state: State<'_, AppState>,
    binding_id: String,
) -> Result<(), String> {
    let bid = Uuid::parse_str(&binding_id).map_err(|e| e.to_string())?;
    let bindings_pool = &state.pool;
    // remove symlink/junction targets before deleting rows
    let installs = list_installations(bindings_pool)
        .await
        .map_err(|e| e.to_string())?;
    for inst in &installs {
        for b in list_skill_bindings(bindings_pool, inst.id)
            .await
            .map_err(|e| e.to_string())?
        {
            if b.id == bid && b.binding_type == BindingType::Symlink {
                let _ = std::fs::remove_file(&b.target_path);
            }
        }
    }
    sqlx::query("DELETE FROM harness_skill_bindings WHERE id = ?")
        .bind(bid.to_string())
        .execute(bindings_pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}
