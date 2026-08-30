//! Sync flow: desired -> actual -> plan -> native plan -> preview/apply -> verify.

use adapters::all_adapters;
use chm_core::domain::harness::{
    BindingType, HarnessInstallation, HarnessMcpBinding, HarnessModelBinding, HarnessSkillBinding,
};
use chm_core::domain::history::{ConfigSnapshot, TransactionStatus, TransactionType};
use chm_database::repos::harness::{list_model_bindings, upsert_model_binding};
use chm_database::repos::history::{add_snapshot, begin_transaction, finish_transaction};
use chm_database::repos::mcp::{list_mcp_bindings, list_mcp_servers, upsert_mcp_binding};
use chm_database::repos::models::list_routes;
use chm_database::repos::skills::{list_skill_bindings, list_skills, upsert_skill_binding};
use chm_filesystem::backup_file;
use chm_harness_sdk::adapter::plan::{
    ActualState, DesiredState, Mode, PlanAction, ReconciliationPlan,
};
use chm_harness_sdk::adapter::types::{ApplyResult, HarnessAdapter, NativePlan, ValidationReport};
use chm_reconciliation::engine::{filter_unsupported, reconcile};
use serde::{Deserialize, Serialize};
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
    pub plan_hash: String,
    pub writable_changes: usize,
    pub has_blockers: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyncSelection {
    #[serde(default)]
    pub model_ids: Vec<String>,
    #[serde(default)]
    pub mcp_ids: Vec<String>,
    #[serde(default)]
    pub skill_ids: Vec<String>,
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

fn effective_mode(mode: &str, selection: Option<&SyncSelection>) -> Result<Mode, String> {
    if selection.is_some() && matches!(parse_mode(mode), Mode::ReplaceManaged) {
        return Err(
            "selection-scoped sync only supports Append; choose the full library scope for Replace Managed"
                .into(),
        );
    }
    Ok(if selection.is_some() {
        Mode::Append
    } else {
        parse_mode(mode)
    })
}

async fn desired_state(
    pool: &Pool<Sqlite>,
    selection: Option<&SyncSelection>,
) -> Result<DesiredState, String> {
    let route_ids = selection.map(|s| s.model_ids.iter().collect::<std::collections::HashSet<_>>());
    let mcp_ids = selection.map(|s| s.mcp_ids.iter().collect::<std::collections::HashSet<_>>());
    let skill_ids = selection.map(|s| s.skill_ids.iter().collect::<std::collections::HashSet<_>>());
    Ok(DesiredState {
        routes: list_routes(pool)
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|r| r.enabled)
            .filter(|r| {
                route_ids
                    .as_ref()
                    .is_none_or(|ids| ids.contains(&r.id.to_string()))
            })
            .collect(),
        mcp_servers: list_mcp_servers(pool)
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|m| m.enabled)
            .filter(|m| {
                mcp_ids
                    .as_ref()
                    .is_none_or(|ids| ids.contains(&m.id.to_string()))
            })
            .collect(),
        skills: list_skills(pool)
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|s| s.enabled)
            .filter(|s| {
                skill_ids
                    .as_ref()
                    .is_none_or(|ids| ids.contains(&s.id.to_string()))
            })
            .collect(),
    })
}

/// managed_flags from the binding tables for this installation. A binding is
/// the durable ownership record used by Replace Managed; native rows without
/// one are deliberately preserved.
async fn managed_flags_for(
    pool: &Pool<Sqlite>,
    install: &HarnessInstallation,
    parsed: &chm_harness_sdk::adapter::types::ParsedState,
) -> Result<std::collections::HashMap<String, bool>, String> {
    let mut flags = std::collections::HashMap::new();
    let bindings = list_model_bindings(pool, install.id)
        .await
        .map_err(|e| e.to_string())?;
    for m in &parsed.models {
        let provider = native_provider_id(&m.route).unwrap_or("");
        let managed = bindings.iter().any(|b| {
            let binding_provider = b
                .native_config
                .get("native_provider_id")
                .and_then(|value| value.as_str())
                .or_else(|| {
                    b.native_config
                        .get("native")
                        .and_then(|value| value.get("native_provider_id"))
                        .and_then(|value| value.as_str())
                });
            b.native_id.eq_ignore_ascii_case(&m.native_id)
                && b.managed
                // A missing provider is only a wildcard for a harness row
                // that also has no provider. Otherwise an old providerless
                // binding could mark duplicate ids under every provider as
                // managed and Replace Managed could remove user-owned rows.
                && match (binding_provider, provider) {
                    (Some(binding), current) => binding.eq_ignore_ascii_case(current),
                    (None, "") => true,
                    (None, _) => false,
                }
        });
        flags.insert(
            format!("route:{}:{}", m.route.endpoint_id, m.native_id),
            managed,
        );
        flags.insert(
            format!("model:{provider}:{}", m.native_id.to_lowercase()),
            managed,
        );
    }
    let mcp_bindings = list_mcp_bindings(pool, install.id)
        .await
        .map_err(|e| e.to_string())?;
    for m in &parsed.mcp {
        flags.insert(
            format!("mcp:{}", m.native_name),
            mcp_bindings
                .iter()
                .any(|b| b.native_name == m.native_name && b.managed),
        );
    }
    let skill_bindings = list_skill_bindings(pool, install.id)
        .await
        .map_err(|e| e.to_string())?;
    for s in &parsed.skills {
        flags.insert(
            format!("skill:{}", s.path),
            skill_bindings
                .iter()
                .any(|b| b.target_path == s.path && b.managed),
        );
    }
    Ok(flags)
}

fn native_provider_id(route: &chm_core::domain::models::ModelRoute) -> Option<&str> {
    route
        .overrides
        .get("native_provider_id")
        .and_then(|v| v.as_str())
        .or_else(|| {
            route
                .overrides
                .get("native")
                .and_then(|v| v.get("native_provider_id"))
                .and_then(|v| v.as_str())
        })
}

/// Persist the ownership records for resources that are present after a
/// successful write. We only bind rows that the adapter can read back, so a
/// no-op/unsupported action never becomes falsely managed.
async fn record_bindings(
    pool: &Pool<Sqlite>,
    install: &HarnessInstallation,
    desired: &DesiredState,
    plan: &ReconciliationPlan,
    parsed: &chm_harness_sdk::adapter::types::ParsedState,
) -> Result<(), String> {
    let now = chrono::Utc::now();
    for route in &desired.routes {
        let changed_by_sync = plan.actions.iter().any(|action| match action {
            PlanAction::Add(add) => {
                add.kind == "model" && add.identity.eq_ignore_ascii_case(&route.remote_model_id)
            }
            PlanAction::Update(update) => {
                update.kind == "model"
                    && update.identity.eq_ignore_ascii_case(&route.remote_model_id)
            }
            _ => false,
        });
        if !changed_by_sync {
            continue;
        }
        let provider = native_provider_id(route);
        if let Some(actual) = parsed.models.iter().find(|m| {
            m.route
                .remote_model_id
                .eq_ignore_ascii_case(&route.remote_model_id)
                && native_provider_id(&m.route)
                    .map(|p| provider.is_some_and(|wanted| p.eq_ignore_ascii_case(wanted)))
                    .unwrap_or(provider.is_none())
        }) {
            upsert_model_binding(
                pool,
                &HarnessModelBinding {
                    id: Uuid::new_v4(),
                    harness_installation_id: install.id,
                    model_route_id: route.id,
                    native_id: actual.native_id.clone(),
                    native_config: serde_json::json!({
                        "native_provider_id": provider,
                        "remote_model_id": route.remote_model_id,
                    }),
                    managed: true,
                    created_at: now,
                    updated_at: now,
                },
            )
            .await
            .map_err(|e| e.to_string())?;
        }
    }
    for server in &desired.mcp_servers {
        let changed_by_sync = plan.actions.iter().any(|action| match action {
            PlanAction::Add(add) => add.kind == "mcp" && add.identity == server.name,
            PlanAction::Update(update) => update.kind == "mcp" && update.identity == server.name,
            _ => false,
        });
        if !changed_by_sync {
            continue;
        }
        if parsed.mcp.iter().any(|m| m.native_name == server.name) {
            upsert_mcp_binding(
                pool,
                &HarnessMcpBinding {
                    id: Uuid::new_v4(),
                    harness_installation_id: install.id,
                    mcp_server_id: server.id,
                    native_name: server.name.clone(),
                    native_config: serde_json::to_value(server).map_err(|e| e.to_string())?,
                    managed: true,
                },
            )
            .await
            .map_err(|e| e.to_string())?;
        }
    }
    for skill in &desired.skills {
        let changed_by_sync = plan.actions.iter().any(|action| match action {
            PlanAction::Add(add) => {
                add.kind == "skill"
                    && (add.identity == skill.canonical_path || add.identity == skill.name)
            }
            PlanAction::Update(update) => {
                update.kind == "skill"
                    && (update.identity == skill.canonical_path || update.identity == skill.name)
            }
            _ => false,
        });
        if !changed_by_sync {
            continue;
        }
        if let Some(actual) = parsed
            .skills
            .iter()
            .find(|s| s.path == skill.canonical_path || s.name == skill.name)
        {
            upsert_skill_binding(
                pool,
                &HarnessSkillBinding {
                    id: Uuid::new_v4(),
                    harness_installation_id: install.id,
                    skill_id: skill.id,
                    target_path: actual.path.clone(),
                    binding_type: if actual.symlinked {
                        BindingType::Symlink
                    } else {
                        BindingType::Copy
                    },
                    managed: true,
                    status: "active".into(),
                },
            )
            .await
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub(crate) fn plan_hash(
    plan: &ReconciliationPlan,
    native_plan: &NativePlan,
) -> Result<String, String> {
    let bytes = serde_json::to_vec(&(plan, native_plan)).map_err(|e| e.to_string())?;
    Ok(crate::drift::sha256_hex_bytes(&bytes))
}

pub(crate) fn validate_apply_request(
    expected_plan_hash: Option<&str>,
    current_plan_hash: &str,
    writable_changes: usize,
    has_blockers: bool,
    force: bool,
) -> Result<(), String> {
    if let Some(expected) = expected_plan_hash
        && expected != current_plan_hash
    {
        return Err(
            "preview is stale: the library or harness changed; refresh before applying".into(),
        );
    }
    if has_blockers && !force {
        return Err(
            "preview contains conflicts or unsupported changes; review them or enable Force".into(),
        );
    }
    if writable_changes == 0 {
        return Err("nothing to apply: preview contains no writable changes".into());
    }
    Ok(())
}

pub(crate) fn action_views(plan: &ReconciliationPlan) -> Vec<ActionView> {
    plan.actions
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
        .collect()
}

/// Render the user-facing preview from the same plan representation used by
/// both regular library sync and configuration-set sync. Keeping this seam in
/// the sync module prevents the two preview commands from drifting in what
/// they expose (especially blockers and writable file counts).
pub(crate) fn preview_report(
    plan: &ReconciliationPlan,
    native_plan: &NativePlan,
) -> Result<PreviewReport, String> {
    let actions = action_views(plan);
    let files = native_plan
        .changes
        .iter()
        .map(|change| FilePreview {
            path: change.file_path.clone(),
            before: change.before.clone(),
            after: change.after.clone(),
        })
        .collect();
    let has_blockers = actions
        .iter()
        .any(|action| action.action == "conflict" || action.action == "unsupported");
    Ok(PreviewReport {
        summary: plan.summary(),
        actions,
        files,
        plan_hash: plan_hash(plan, native_plan)?,
        writable_changes: native_plan.changes.len(),
        has_blockers,
    })
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
    build_native_plan_scoped(pool, installation_id, mode, None).await
}

pub async fn build_native_plan_scoped(
    pool: &Pool<Sqlite>,
    installation_id: &str,
    mode: &Mode,
    selection: Option<&SyncSelection>,
) -> Result<
    (
        HarnessInstallation,
        Box<dyn HarnessAdapter>,
        ReconciliationPlan,
        NativePlan,
    ),
    String,
> {
    let desired = desired_state(pool, selection).await?;
    build_native_plan_for_desired(pool, installation_id, mode, desired).await
}

/// Build a native plan from an explicit desired state. Configuration sets use
/// this same path so their preview and apply semantics cannot drift from the
/// normal library sync flow.
pub async fn build_native_plan_for_desired(
    pool: &Pool<Sqlite>,
    installation_id: &str,
    mode: &Mode,
    desired: DesiredState,
) -> Result<
    (
        HarnessInstallation,
        Box<dyn HarnessAdapter>,
        ReconciliationPlan,
        NativePlan,
    ),
    String,
> {
    let inst = crate::commands::find_installation(pool, installation_id).await?;
    let adapter = adapter_for(inst.harness_type.as_str()).ok_or("no adapter")?;
    let parsed = adapter.read_state(&inst).map_err(|e| e.to_string())?;
    let actual = ActualState {
        routes: parsed.models.clone(),
        mcp: parsed.mcp.clone(),
        skills: parsed.skills.clone(),
        managed_flags: managed_flags_for(pool, &inst, &parsed).await?,
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
    selection: Option<SyncSelection>,
) -> Result<PreviewReport, String> {
    let m = effective_mode(&mode, selection.as_ref())?;
    let (_, _, plan, native_plan) =
        build_native_plan_scoped(&state.pool, &installation_id, &m, selection.as_ref()).await?;
    preview_report(&plan, &native_plan)
}

pub async fn execute_sync(
    pool: &Pool<Sqlite>,
    installation_id: &str,
    mode: &Mode,
    force: bool,
) -> Result<ApplyReport, String> {
    execute_sync_with_plan(pool, installation_id, mode, force, None, None).await
}

pub async fn execute_sync_with_plan(
    pool: &Pool<Sqlite>,
    installation_id: &str,
    mode: &Mode,
    force: bool,
    expected_plan_hash: Option<&str>,
    selection: Option<&SyncSelection>,
) -> Result<ApplyReport, String> {
    let desired = desired_state(pool, selection).await?;
    execute_desired_with_plan(
        pool,
        installation_id,
        mode,
        force,
        expected_plan_hash,
        selection,
        desired,
    )
    .await
}

/// Execute a validated native plan built from an explicit desired state.
/// Configuration sets call this instead of maintaining a second transaction
/// implementation, keeping backup, snapshot, validation, rollback, and
/// ownership semantics identical to normal sync.
pub async fn execute_desired_with_plan(
    pool: &Pool<Sqlite>,
    installation_id: &str,
    mode: &Mode,
    force: bool,
    expected_plan_hash: Option<&str>,
    selection: Option<&SyncSelection>,
    desired: DesiredState,
) -> Result<ApplyReport, String> {
    if selection.is_some() && matches!(mode, Mode::ReplaceManaged) {
        return Err(
            "selection-scoped sync only supports Append; choose the full library scope for Replace Managed"
                .into(),
        );
    }
    let effective_mode = if selection.is_some() {
        Mode::Append
    } else {
        *mode
    };
    let (inst, adapter, _plan, native_plan) =
        build_native_plan_for_desired(pool, installation_id, &effective_mode, desired.clone())
            .await?;
    let current_hash = plan_hash(&_plan, &native_plan)?;
    let blockers = _plan
        .actions
        .iter()
        .any(|action| matches!(action, PlanAction::Conflict(_) | PlanAction::Unsupported(_)));
    let writable_changes = native_plan.changes.len();
    validate_apply_request(
        expected_plan_hash,
        &current_hash,
        writable_changes,
        blockers,
        force,
    )?;
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
                    std::slice::from_ref(&msg),
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
            let hash = crate::drift::sha256_hex;
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
                        let desired_after = desired.clone();
                        let parsed_after = match adapter.read_state(&inst) {
                            Ok(parsed) => parsed,
                            Err(error) => {
                                let message = error.to_string();
                                rollback_all(
                                    pool,
                                    tx.id,
                                    &*adapter,
                                    &inst,
                                    &native_plan,
                                    &backups,
                                    std::slice::from_ref(&message),
                                )
                                .await?;
                                return Err(format!(
                                    "sync validation passed but the resulting state could not be read; rolled back: {message}"
                                ));
                            }
                        };
                        if let Err(error) =
                            record_bindings(pool, &inst, &desired_after, &_plan, &parsed_after)
                                .await
                        {
                            rollback_all(
                                pool,
                                tx.id,
                                &*adapter,
                                &inst,
                                &native_plan,
                                &backups,
                                std::slice::from_ref(&error),
                            )
                            .await?;
                            return Err(format!(
                                "sync succeeded on disk but ownership could not be recorded; rolled back: {error}"
                            ));
                        }
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
                std::slice::from_ref(&e),
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
    crate::services::transactions::rollback_native_transaction(
        pool,
        tx_id,
        adapter,
        inst,
        native_plan,
        backups,
        errors,
    )
    .await
}

#[tauri::command]
pub async fn sync_apply(
    state: State<'_, AppState>,
    installation_id: String,
    mode: String,
    force: bool,
    plan_hash: String,
    selection: Option<SyncSelection>,
) -> Result<ApplyReport, String> {
    execute_sync_with_plan(
        &state.pool,
        &installation_id,
        &parse_mode(&mode),
        force,
        Some(&plan_hash),
        selection.as_ref(),
    )
    .await
}

/// Syncs ONE canonical MCP server into a harness's native config using the
/// full sync machinery (backups, snapshots, verify, rollback).
pub async fn bind_mcp_sync(
    pool: &Pool<Sqlite>,
    inst: &HarnessInstallation,
    server: &chm_core::domain::mcp::McpServer,
) -> Result<(), String> {
    let desired = DesiredState {
        mcp_servers: vec![server.clone()],
        ..Default::default()
    };
    let installation_id = inst.id.to_string();
    let (_, _adapter, plan, native_plan) =
        build_native_plan_for_desired(pool, &installation_id, &Mode::Append, desired.clone())
            .await?;
    if native_plan.changes.is_empty() {
        return Ok(());
    }
    let expected_hash = plan_hash(&plan, &native_plan)?;
    execute_desired_with_plan(
        pool,
        &installation_id,
        &Mode::Append,
        false,
        Some(&expected_hash),
        None,
        desired,
    )
    .await
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::{SyncSelection, effective_mode, validate_apply_request};
    use chm_harness_sdk::adapter::plan::Mode;

    #[test]
    fn stale_preview_is_rejected_before_writes() {
        let error = validate_apply_request(Some("old"), "new", 1, false, false).unwrap_err();
        assert!(error.contains("stale"));
    }

    #[test]
    fn no_op_preview_cannot_apply() {
        let error = validate_apply_request(Some("same"), "same", 0, false, false).unwrap_err();
        assert!(error.contains("no writable"));
    }

    #[test]
    fn blockers_need_force_and_force_is_explicit() {
        let error = validate_apply_request(Some("same"), "same", 1, true, false).unwrap_err();
        assert!(error.contains("conflicts"));
        assert!(validate_apply_request(Some("same"), "same", 1, true, true).is_ok());
    }

    #[test]
    fn selection_scope_rejects_replace_managed_instead_of_silently_downgrading() {
        let selection = SyncSelection {
            model_ids: vec!["route".into()],
            ..Default::default()
        };
        let error = effective_mode("replaceManaged", Some(&selection)).unwrap_err();
        assert!(error.contains("selection-scoped"));
        assert!(matches!(
            effective_mode("append", Some(&selection)),
            Ok(Mode::Append)
        ));
    }
}
