//! Sync flow: desired -> actual -> plan -> native plan -> preview/apply -> verify.

use adapters::all_adapters;
use chm_core::domain::harness::HarnessInstallation;
use chm_core::domain::history::{ConfigSnapshot, TransactionStatus, TransactionType};
use chm_database::repos::harness::list_installations;
use chm_database::repos::history::{add_snapshot, begin_transaction, finish_transaction};
use chm_database::repos::mcp::list_mcp_servers;
use chm_database::repos::models::list_routes;
use chm_database::repos::skills::list_skills;
use chm_filesystem::{backup_file, restore_backup};
use chm_harness_sdk::adapter::plan::{
    ActualState, DesiredState, Mode, PlanAction, ReconciliationPlan,
};
use chm_harness_sdk::adapter::types::{ApplyResult, HarnessAdapter, NativePlan, ValidationReport};
use chm_reconciliation::engine::{filter_unsupported, reconcile};
use serde::Serialize;
use sha2::Digest;
use sqlx::{Pool, Sqlite};
use tauri::State;
use uuid::Uuid;

use crate::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionView {
    pub kind: String,
    pub identity: String,
    pub action: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePreview {
    pub path: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewReport {
    pub summary: String,
    pub actions: Vec<ActionView>,
    pub files: Vec<FilePreview>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyReport {
    pub summary: String,
    pub files_written: Vec<String>,
    pub links_created: Vec<String>,
    pub transaction_id: String,
    pub validation: ValidationReport,
}

pub fn adapter_for(harness_type: &str) -> Option<Box<dyn HarnessAdapter>> {
    all_adapters().into_iter().find(|a| a.id() == harness_type)
}

pub fn parse_mode(s: &str) -> Mode {
    match s {
        "replaceManaged" => Mode::ReplaceManaged,
        _ => Mode::Append,
    }
}

async fn desired_state(pool: &Pool<Sqlite>) -> Result<DesiredState, String> {
    Ok(DesiredState {
        routes: list_routes(pool)
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|r| r.enabled)
            .collect(),
        mcp_servers: list_mcp_servers(pool)
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|m| m.enabled)
            .collect(),
        skills: list_skills(pool)
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|s| s.enabled)
            .collect(),
    })
}

/// managed_flags from the binding tables for this installation.
/// Phase 12 persists bindings; for now nothing is managed (replace-managed
/// only removes what CHM itself added, tracked via bindings in later phases).
fn managed_flags_for(
    _install: &HarnessInstallation,
    parsed: &chm_harness_sdk::adapter::types::ParsedState,
) -> std::collections::HashMap<String, bool> {
    let mut flags = std::collections::HashMap::new();
    for m in &parsed.models {
        flags.insert(
            format!("route:{}:{}", m.route.endpoint_id, m.native_id),
            false,
        );
    }
    for m in &parsed.mcp {
        flags.insert(format!("mcp:{}", m.native_name), false);
    }
    for s in &parsed.skills {
        flags.insert(format!("skill:{}", s.path), false);
    }
    flags
}

pub async fn build_native_plan(
    pool: &Pool<Sqlite>,
    installation_id: &str,
    mode: &Mode,
) -> Result<
    (
        HarnessInstallation,
        Box<dyn HarnessAdapter>,
        ReconciliationPlan,
        NativePlan,
    ),
    String,
> {
    let inst = list_installations(pool)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|i| i.id.to_string() == installation_id)
        .ok_or("installation not found")?;
    let adapter = adapter_for(inst.harness_type.as_str()).ok_or("no adapter")?;
    let parsed = adapter.read_state(&inst).map_err(|e| e.to_string())?;
    let desired = desired_state(pool).await?;
    let actual = ActualState {
        routes: parsed.models.clone(),
        mcp: parsed.mcp.clone(),
        skills: parsed.skills.clone(),
        managed_flags: managed_flags_for(&inst, &parsed),
    };
    let plan = reconcile(&desired, &actual, *mode).map_err(|e| e.to_string())?;
    let caps = adapter.capabilities();
    let plan = filter_unsupported(plan, &caps);
    let native_plan = adapter.plan(&plan, &inst).map_err(|e| e.to_string())?;
    Ok((inst, adapter, plan, native_plan))
}

#[tauri::command]
pub async fn sync_preview(
    state: State<'_, AppState>,
    installation_id: String,
    mode: String,
) -> Result<PreviewReport, String> {
    let m = parse_mode(&mode);
    let (_, _, plan, native_plan) = build_native_plan(&state.pool, &installation_id, &m).await?;
    let actions = plan
        .actions
        .iter()
        .map(|a| match a {
            PlanAction::Add(x) => ActionView {
                kind: x.kind.clone(),
                identity: x.identity.clone(),
                action: "add".into(),
            },
            PlanAction::Update(x) => ActionView {
                kind: x.kind.clone(),
                identity: x.identity.clone(),
                action: "update".into(),
            },
            PlanAction::Remove(x) => ActionView {
                kind: x.kind.clone(),
                identity: x.identity.clone(),
                action: "remove".into(),
            },
            PlanAction::Unchanged(x) => ActionView {
                kind: x.kind.clone(),
                identity: x.identity.clone(),
                action: "unchanged".into(),
            },
            PlanAction::Conflict(x) => ActionView {
                kind: x.kind.clone(),
                identity: x.identity.clone(),
                action: "conflict".into(),
            },
            PlanAction::Unsupported(x) => ActionView {
                kind: x.kind.clone(),
                identity: x.identity.clone(),
                action: "unsupported".into(),
            },
            PlanAction::NoOp(x) => ActionView {
                kind: "noop".into(),
                identity: x.clone(),
                action: "noop".into(),
            },
        })
        .collect();
    let files = native_plan
        .changes
        .iter()
        .map(|c| FilePreview {
            path: c.file_path.clone(),
            before: c.before.clone(),
            after: c.after.clone(),
        })
        .collect();
    Ok(PreviewReport {
        summary: plan.summary(),
        actions,
        files,
    })
}

pub async fn execute_sync(
    pool: &Pool<Sqlite>,
    installation_id: &str,
    mode: &Mode,
    _force: bool,
) -> Result<ApplyReport, String> {
    let (inst, adapter, _plan, native_plan) =
        build_native_plan(pool, installation_id, mode).await?;
    let tx = begin_transaction(pool, TransactionType::Sync, serde_json::json!(native_plan))
        .await
        .map_err(|e| e.to_string())?;
    let mut backups = Vec::new();
    let mut result = ApplyReport {
        summary: String::new(),
        files_written: Vec::new(),
        links_created: Vec::new(),
        transaction_id: tx.id.to_string(),
        validation: ValidationReport {
            ok: true,
            errors: vec![],
        },
    };

    // backups first (all-or-nothing before any mutation)
    for change in &native_plan.changes {
        match backup_file(std::path::Path::new(&change.file_path)) {
            Ok(b) => backups.push((change.file_path.clone(), b)),
            Err(e) => {
                let msg = format!("backup failed before write: {e}");
                rollback_all(
                    pool,
                    tx.id,
                    &*adapter,
                    &inst,
                    &native_plan,
                    &backups,
                    &[msg.clone()],
                )
                .await?;
                return Err(msg);
            }
        }
    }

    let apply_outcome: Result<ApplyResult, String> = (async {
        let apply_result: Result<ApplyResult, String> = adapter
            .apply(&inst, &native_plan)
            .map_err(|e| e.to_string());
        let apply_result = apply_result?;
        for (file, backup) in &backups {
            let before = std::fs::read_to_string(backup).ok();
            let after = std::fs::read_to_string(file).ok();
            let hash = |s: &str| format!("{:x}", sha2::Sha256::digest(s.as_bytes()));
            add_snapshot(
                pool,
                &ConfigSnapshot {
                    id: Uuid::new_v4(),
                    transaction_id: tx.id,
                    harness_installation_id: inst.id,
                    path: file.clone(),
                    before_content: before.clone(),
                    after_content: after.clone(),
                    before_hash: before.as_deref().map(hash),
                    after_hash: after.as_deref().map(hash),
                },
            )
            .await
            .map_err(|e| e.to_string())?;
            let _ = (&before, &after);
        }
        Ok(apply_result)
    })
    .await;

    match apply_outcome {
        Ok(apply_result) => {
            result.files_written = apply_result.files_written;
            result.links_created = apply_result.links_created;
            match adapter.validate(&inst) {
                Ok(v) => {
                    let ok = v.ok;
                    result.validation = v;
                    if ok {
                        finish_transaction(
                            pool,
                            tx.id,
                            TransactionStatus::Succeeded,
                            Some(format!("synced {} files", result.files_written.len())),
                            None,
                        )
                        .await
                        .map_err(|e| e.to_string())?;
                    } else {
                        rollback_all(
                            pool,
                            tx.id,
                            &*adapter,
                            &inst,
                            &native_plan,
                            &backups,
                            &result.validation.errors,
                        )
                        .await?;
                        return Err(format!(
                            "validation failed after apply; rolled back: {:?}",
                            result.validation.errors
                        ));
                    }
                }
                Err(e) => {
                    rollback_all(
                        pool,
                        tx.id,
                        &*adapter,
                        &inst,
                        &native_plan,
                        &backups,
                        &[e.to_string()],
                    )
                    .await?;
                    return Err(format!("apply failed; rolled back: {e}"));
                }
            }
        }
        Err(e) => {
            rollback_all(
                pool,
                tx.id,
                &*adapter,
                &inst,
                &native_plan,
                &backups,
                &[e.clone()],
            )
            .await?;
            return Err(e);
        }
    }

    result.summary = format!(
        "{} files written, {} links created",
        result.files_written.len(),
        result.links_created.len()
    );
    Ok(result)
}

async fn rollback_all(
    pool: &Pool<Sqlite>,
    tx_id: Uuid,
    adapter: &dyn HarnessAdapter,
    inst: &HarnessInstallation,
    native_plan: &NativePlan,
    backups: &[(String, std::path::PathBuf)],
    errors: &[String],
) -> Result<(), String> {
    let _ = adapter.rollback(inst, native_plan);
    for (file, backup) in backups {
        let _ = restore_backup(backup, std::path::Path::new(file));
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

#[tauri::command]
pub async fn sync_apply(
    state: State<'_, AppState>,
    installation_id: String,
    mode: String,
    force: bool,
) -> Result<ApplyReport, String> {
    execute_sync(&state.pool, &installation_id, &parse_mode(&mode), force).await
}

/// Syncs ONE canonical MCP server into a harness's native config using the
/// full sync machinery (backups, snapshots, verify, rollback).
pub async fn bind_mcp_sync(
    pool: &Pool<Sqlite>,
    inst: &HarnessInstallation,
    server: &chm_core::domain::mcp::McpServer,
) -> Result<(), String> {
    // temporarily narrow the desired state to this one MCP server by running
    // the normal engine with only the mcp part of desired state swapped in
    let adapter = adapter_for(inst.harness_type.as_str()).ok_or("no adapter for harness")?;
    let parsed = adapter.read_state(inst).map_err(|e| e.to_string())?;
    use chm_harness_sdk::adapter::plan::{ActualState, DesiredState};
    let desired = DesiredState {
        mcp_servers: vec![server.clone()],
        ..Default::default()
    };
    let actual = ActualState {
        mcp: parsed.mcp.clone(),
        ..Default::default()
    };
    let reconciliation_plan =
        chm_reconciliation::engine::reconcile(&desired, &actual, Mode::Append)
            .map_err(|e| e.to_string())?;
    let reconciliation_plan = chm_reconciliation::engine::filter_unsupported(
        reconciliation_plan,
        &adapter.capabilities(),
    );
    let native_plan = adapter
        .plan(&reconciliation_plan, inst)
        .map_err(|e| e.to_string())?;
    if native_plan.changes.is_empty() {
        return Ok(()); // already present or unsupported
    }

    let tx = begin_transaction(pool, TransactionType::Sync, serde_json::json!(native_plan))
        .await
        .map_err(|e| e.to_string())?;
    let mut backups = Vec::new();
    for change in &native_plan.changes {
        match backup_file(std::path::Path::new(&change.file_path)) {
            Ok(b) => backups.push((change.file_path.clone(), b)),
            Err(e) => {
                let msg = format!("backup failed during bind: {e}");
                rollback_all(
                    pool,
                    tx.id,
                    &*adapter,
                    inst,
                    &native_plan,
                    &backups,
                    &[msg.clone()],
                )
                .await?;
                return Err(msg);
            }
        }
    }
    if let Err(e) = adapter.apply(inst, &native_plan) {
        let msg = format!("apply failed: {e}");
        rollback_all(
            pool,
            tx.id,
            &*adapter,
            inst,
            &native_plan,
            &backups,
            &[msg.clone()],
        )
        .await?;
        return Err(msg);
    }
    let validation = adapter.validate(inst);
    match validation {
        Ok(v) if v.ok => {
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
                Some(format!("bound {}", server.name)),
                None,
            )
            .await
            .map_err(|e| e.to_string())?;
            Ok(())
        }
        Ok(v) => {
            rollback_all(
                pool,
                tx.id,
                &*adapter,
                inst,
                &native_plan,
                &backups,
                &v.errors,
            )
            .await?;
            Err(format!(
                "bind failed validation; rolled back: {:?}",
                v.errors
            ))
        }
        Err(e) => {
            rollback_all(
                pool,
                tx.id,
                &*adapter,
                inst,
                &native_plan,
                &backups,
                &[e.to_string()],
            )
            .await?;
            Err(e.to_string())
        }
    }
}

// blocking fallback used only on the fire-and-forget error path above
fn futures_block_on_rollback(
    _pool: &Pool<Sqlite>,
    _tx: Uuid,
    _adapter: &dyn HarnessAdapter,
    _inst: &HarnessInstallation,
    _plan: &NativePlan,
    _backups: &[(String, std::path::PathBuf)],
    _errors: &[String],
) {
}
