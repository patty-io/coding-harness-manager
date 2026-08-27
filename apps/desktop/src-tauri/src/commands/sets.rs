//! Configuration set commands.

use chm_core::domain::sets::{ConfigurationSetItem, SetItemType};
use chm_database::repos::models::list_routes;
use chm_database::repos::profiles::{add_set_item, list_set_items, list_sets};
use serde::Serialize;
use sqlx::{Pool, Sqlite};
use tauri::State;
use uuid::Uuid;

use crate::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetItemView {
    pub item_type: String,
    pub item_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetView {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub items: Vec<SetItemView>,
}

#[tauri::command]
pub async fn list_sets_cmd(state: State<'_, AppState>) -> Result<Vec<SetView>, String> {
    let pool = &state.pool;
    let sets = list_sets(pool).await.map_err(|e| e.to_string())?;
    let mut views = Vec::new();
    for s in sets {
        let items = list_set_items(pool, s.id)
            .await
            .map_err(|e| e.to_string())?;
        views.push(SetView {
            id: s.id.to_string(),
            name: s.name,
            description: s.description,
            items: items
                .into_iter()
                .map(|i| SetItemView {
                    item_type: i.item_type.as_str().to_string(),
                    item_id: i.item_id.to_string(),
                })
                .collect(),
        });
    }
    Ok(views)
}

#[tauri::command]
pub async fn create_set_cmd(
    state: State<'_, AppState>,
    name: String,
    description: Option<String>,
) -> Result<String, String> {
    let set = chm_database::repos::profiles::create_set(&state.pool, &name, description)
        .await
        .map_err(|e| e.to_string())?;
    Ok(set.id.to_string())
}

#[tauri::command]
pub async fn add_set_item_cmd(
    state: State<'_, AppState>,
    set_id: String,
    item_type: String,
    item_id: String,
) -> Result<(), String> {
    let set_id = Uuid::parse_str(&set_id).map_err(|e| e.to_string())?;
    let item_id = Uuid::parse_str(&item_id).map_err(|e| e.to_string())?;
    add_set_item(
        &state.pool,
        set_id,
        SetItemType::parse_str(&item_type),
        item_id,
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn remove_set_item_cmd(
    state: State<'_, AppState>,
    set_id: String,
    item_type: String,
    item_id: String,
) -> Result<(), String> {
    let pool = &state.pool;
    let sid = Uuid::parse_str(&set_id).map_err(|e| e.to_string())?;
    let iid = Uuid::parse_str(&item_id).map_err(|e| e.to_string())?;
    let itype = SetItemType::parse_str(&item_type);
    let items = list_set_items(pool, sid).await.map_err(|e| e.to_string())?;
    for item in items {
        if item.item_type == itype && item.item_id == iid {
            sqlx::query("DELETE FROM configuration_set_items WHERE id = ?")
                .bind(item.id.to_string())
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
            return Ok(());
        }
    }
    Ok(())
}

/// DesiredState limited to the set's members (testable core).
pub async fn set_filtered_desired(
    pool: &Pool<Sqlite>,
    set_id: &str,
) -> Result<chm_harness_sdk::adapter::plan::DesiredState, String> {
    let sid = Uuid::parse_str(set_id).map_err(|e| e.to_string())?;
    let items = list_set_items(pool, sid).await.map_err(|e| e.to_string())?;
    let routes = list_routes(pool).await.map_err(|e| e.to_string())?;
    let mcp = chm_database::repos::mcp::list_mcp_servers(pool)
        .await
        .map_err(|e| e.to_string())?;
    let skills = chm_database::repos::skills::list_skills(pool)
        .await
        .map_err(|e| e.to_string())?;

    Ok(chm_harness_sdk::adapter::plan::DesiredState {
        routes: routes
            .into_iter()
            .filter(|r| {
                items
                    .iter()
                    .any(|i| i.item_type == SetItemType::ModelRoute && i.item_id == r.id)
            })
            .collect(),
        mcp_servers: mcp
            .into_iter()
            .filter(|s| {
                items
                    .iter()
                    .any(|i| i.item_type == SetItemType::McpServer && i.item_id == s.id)
            })
            .collect(),
        skills: skills
            .into_iter()
            .filter(|sk| {
                items
                    .iter()
                    .any(|i| i.item_type == SetItemType::Skill && i.item_id == sk.id)
            })
            .collect(),
    })
}
/// Preview applying a set: same machinery as sync_preview with set-filtered desired.
use chm_harness_sdk::adapter::plan::Mode;

#[tauri::command]
pub async fn apply_set_preview_cmd(
    state: State<'_, AppState>,
    set_id: String,
    installation_id: String,
) -> Result<crate::commands::sync::PreviewReport, String> {
    let pool = &state.pool;
    let inst = chm_database::repos::harness::list_installations(pool)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|i| i.id.to_string() == installation_id)
        .ok_or("installation not found")?;
    let adapter =
        crate::commands::sync::adapter_for(inst.harness_type.as_str()).ok_or("no adapter")?;
    let parsed = adapter.read_state(&inst).map_err(|e| e.to_string())?;
    let desired = set_filtered_desired(pool, &set_id).await?;
    let actual = chm_harness_sdk::adapter::plan::ActualState {
        routes: parsed.models.clone(),
        mcp: parsed.mcp.clone(),
        skills: parsed.skills.clone(),
        managed_flags: Default::default(),
    };
    let plan = chm_reconciliation::engine::reconcile(&desired, &actual, Mode::Append)
        .map_err(|e| e.to_string())?;
    let plan = chm_reconciliation::engine::filter_unsupported(plan, &adapter.capabilities());
    let native_plan = adapter.plan(&plan, &inst).map_err(|e| e.to_string())?;
    Ok(crate::commands::sync::PreviewReport {
        summary: plan.summary(),
        actions: plan
            .actions
            .iter()
            .map(|a| match a {
                chm_harness_sdk::adapter::plan::PlanAction::Add(x) => {
                    crate::commands::sync::ActionView {
                        kind: x.kind.clone(),
                        identity: x.identity.clone(),
                        action: "add".into(),
                    }
                }
                _ => crate::commands::sync::ActionView {
                    kind: String::new(),
                    identity: String::new(),
                    action: "noop".into(),
                },
            })
            .collect(),
        files: native_plan
            .changes
            .iter()
            .map(|c| crate::commands::sync::FilePreview {
                path: c.file_path.clone(),
                before: c.before.clone(),
                after: c.after.clone(),
            })
            .collect(),
    })
}

#[tauri::command]
pub async fn apply_set_cmd(
    state: State<'_, AppState>,
    set_id: String,
    installation_id: String,
    mode: String,
) -> Result<crate::commands::sync::ApplyReport, String> {
    // reuse execute_sync but with set-filtered desired state: simplest correct
    // approach is to run execute_sync after narrowing enabled routes — V1 runs
    // a scoped rebuild inline here.
    let pool = &state.pool;
    let m = match mode.as_str() {
        "replaceManaged" => Mode::ReplaceManaged,
        _ => Mode::Append,
    };
    let inst = chm_database::repos::harness::list_installations(pool)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|i| i.id.to_string() == installation_id)
        .ok_or("installation not found")?;
    let adapter =
        crate::commands::sync::adapter_for(inst.harness_type.as_str()).ok_or("no adapter")?;
    let parsed = adapter.read_state(&inst).map_err(|e| e.to_string())?;
    let desired = set_filtered_desired(pool, &set_id).await?;
    let actual = chm_harness_sdk::adapter::plan::ActualState {
        routes: parsed.models.clone(),
        mcp: parsed.mcp.clone(),
        skills: parsed.skills.clone(),
        managed_flags: Default::default(),
    };
    let plan =
        chm_reconciliation::engine::reconcile(&desired, &actual, m).map_err(|e| e.to_string())?;
    let plan = chm_reconciliation::engine::filter_unsupported(plan, &adapter.capabilities());
    let native_plan = adapter.plan(&plan, &inst).map_err(|e| e.to_string())?;

    use chm_core::domain::history::{ConfigSnapshot, TransactionStatus, TransactionType};
    use chm_database::repos::history::{add_snapshot, begin_transaction, finish_transaction};
    let tx = begin_transaction(pool, TransactionType::Sync, serde_json::json!(native_plan))
        .await
        .map_err(|e| e.to_string())?;
    let mut backups = Vec::new();
    for change in &native_plan.changes {
        match chm_filesystem::backup_file(std::path::Path::new(&change.file_path)) {
            Ok(b) => backups.push((change.file_path.clone(), b)),
            Err(e) => {
                finish_transaction(
                    pool,
                    tx.id,
                    TransactionStatus::Failed,
                    None,
                    Some(e.to_string()),
                )
                .await
                .map_err(|err| err.to_string())?;
                return Err(e.to_string());
            }
        }
    }
    if let Err(e) = adapter.apply(&inst, &native_plan) {
        rollback_set(
            pool,
            tx.id,
            &*adapter,
            &inst,
            &native_plan,
            &backups,
            &[e.to_string()],
        )
        .await?;
        return Err(format!("apply failed: {e}"));
    }
    let validation = adapter.validate(&inst).map_err(|e| e.to_string())?;
    if !validation.ok {
        rollback_set(
            pool,
            tx.id,
            &*adapter,
            &inst,
            &native_plan,
            &backups,
            &validation.errors,
        )
        .await?;
        return Err(format!("validation failed: {:?}", validation.errors));
    }
    for (file, backup) in &backups {
        let _ = add_snapshot(
            pool,
            &ConfigSnapshot {
                id: Uuid::new_v4(),
                transaction_id: tx.id,
                harness_installation_id: inst.id,
                path: file.clone(),
                before_content: std::fs::read_to_string(backup).ok(),
                after_content: std::fs::read_to_string(file).ok(),
                before_hash: None,
                after_hash: None,
            },
        )
        .await;
    }
    finish_transaction(
        pool,
        tx.id,
        TransactionStatus::Succeeded,
        Some(format!("set applied to {}", inst.harness_type.as_str())),
        None,
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok(crate::commands::sync::ApplyReport {
        summary: format!("{} files written", backups.len()),
        files_written: backups.into_iter().map(|(f, _)| f).collect(),
        links_created: vec![],
        transaction_id: tx.id.to_string(),
        validation,
    })
}

use chm_core::domain::history::TransactionStatus;
use chm_database::repos::history::finish_transaction;

async fn rollback_set(
    pool: &Pool<Sqlite>,
    tx_id: Uuid,
    adapter: &dyn chm_harness_sdk::adapter::types::HarnessAdapter,
    inst: &chm_core::domain::harness::HarnessInstallation,
    native_plan: &chm_harness_sdk::adapter::types::NativePlan,
    backups: &[(String, std::path::PathBuf)],
    errors: &[String],
) -> Result<(), String> {
    let _ = adapter.rollback(inst, native_plan);
    for (file, backup) in backups {
        let _ = chm_filesystem::restore_backup(backup, std::path::Path::new(file));
    }
    finish_transaction(
        pool,
        tx_id,
        TransactionStatus::Failed,
        None,
        Some(errors.join("; ")),
    )
    .await
    .map_err(|e| e.to_string())
}
