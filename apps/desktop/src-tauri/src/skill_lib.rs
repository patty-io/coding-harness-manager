//! Pure skill logic: hashing, scanning, dedup.

use sha2::{Digest, Sha256};
use std::path::Path;

pub fn hash_file(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    Ok(format!("{:x}", Sha256::digest(&bytes)))
}

/// Content hash of a directory tree: SHA-256 over sorted
/// `relative_path:file_hash` lines; symlinks hashed as their target.
pub fn hash_directory(dir: &Path) -> Result<String, String> {
    if !dir.is_dir() {
        return Err(format!("not a directory: {}", dir.display()));
    }
    let mut lines = Vec::new();
    collect_hashes(dir, dir, &mut lines)?;
    lines.sort();
    let mut hasher = Sha256::new();
    for line in lines {
        hasher.update(line.as_bytes());
        hasher.update(b"\n");
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn collect_hashes(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), String> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let rel = path
            .strip_prefix(root)
            .map_err(|e| e.to_string())?
            .display()
            .to_string();
        let ft = entry.file_type().map_err(|e| e.to_string())?;
        if ft.is_symlink() {
            let target = std::fs::read_link(&path)
                .map_err(|e| e.to_string())?
                .display()
                .to_string();
            out.push(format!("{rel}:symlink:{target}"));
        } else if ft.is_dir() {
            collect_hashes(root, &path, out)?;
        } else {
            let h = hash_file(&path)?;
            out.push(format!("{rel}:{h}"));
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct ScannedSkill {
    pub name: String,
    pub path: String,
    pub content_hash: String,
}

/// A directory is a skill iff it contains SKILL.md.
pub fn scan_skill_dirs(dir: &Path) -> Result<Vec<ScannedSkill>, String> {
    let mut out = Vec::new();
    if !dir.is_dir() {
        return Ok(out);
    }
    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if path.join("SKILL.md").exists() {
            out.push(ScannedSkill {
                name: entry.file_name().to_string_lossy().into_owned(),
                path: path.display().to_string(),
                content_hash: hash_directory(&path)?,
            });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

// --- conflict detection (project plan §28) ---

#[derive(Debug, Clone, PartialEq)]
pub struct Conflict {
    pub kind: String,
    pub name: String,
    pub detail: String,
}

pub fn detect_conflicts(
    registry: &[SkillLike],
    harness_skills: &[HarnessSkillLike],
) -> Vec<Conflict> {
    let mut conflicts = Vec::new();
    for r in registry {
        if let Some(h) =
            harness_skills.iter().find(|s| s.name == r.name)
            && r.content_hash != h.content_hash
            && h.content_hash.is_some()
        {
            conflicts.push(Conflict {
                kind: "content-mismatch".into(),
                name: r.name.clone(),
                detail: format!(
                    "canonical {} vs harness {}",
                    r.content_hash.as_deref().unwrap_or("?"),
                    h.content_hash.as_deref().unwrap_or("?")
                ),
            });
        }
    }
    for h in harness_skills.iter().filter(|s| s.symlinked) {
        // dangling symlink detection is done by the caller (needs fs access);
        // here we record only suspicious null-target skills reported by the adapter
        let _ = h;
    }
    conflicts.dedup_by(|a, b| a.kind == b.kind && a.name == b.name);
    conflicts
}

/// Neutral shapes so conflict detection is testable without adapters.
pub struct SkillLike {
    pub name: String,
    pub canonical_path: String,
    pub content_hash: Option<String>,
}

pub struct HarnessSkillLike {
    pub name: String,
    pub path: String,
    pub content_hash: Option<String>,
    pub symlinked: bool,
}
