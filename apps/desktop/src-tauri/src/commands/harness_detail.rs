//! Harness-detail model view: disk rows enriched with library linkage, plus
//! adoption of on-device-only models into the library.

use chm_database::repos::models::{create_route, list_catalog_models, list_routes};
use chm_database::repos::providers::{list_endpoints, list_providers};
use serde::Serialize;
use tauri::State;
use uuid::Uuid;

use crate::AppState;
use crate::commands::import::read_parsed_state;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessModelRow {
    pub native_id: String,
    pub remote_model_id: String,
    pub display_name: String,
    pub context_window: Option<i64>,
    pub in_library: bool,
    pub library_route_id: Option<String>,
    pub library_display_name: Option<String>,
    /// Provider serving this model, when we can attribute it.
    pub provider_name: Option<String>,
    /// How the provider was attributed: "library" (routed My Model) or
    /// "catalog" (exact remote id found in a provider's discovered catalog).
    pub provider_match: Option<String>,
}

#[tauri::command]
pub async fn harness_models_view_cmd(
    state: State<'_, AppState>,
    installation_id: String,
) -> Result<Vec<HarnessModelRow>, String> {
    let (_id, _htype, parsed) = read_parsed_state(&state.pool, &installation_id).await?;
    let routes = list_routes(&state.pool).await.map_err(|e| e.to_string())?;
    let providers = list_providers(&state.pool)
        .await
        .map_err(|e| e.to_string())?;

    // endpoint -> (provider display name, endpoint id) for attribution.
    let mut endpoint_provider: std::collections::HashMap<uuid::Uuid, String> =
        std::collections::HashMap::new();
    for p in &providers {
        for e in list_endpoints(&state.pool, p.id)
            .await
            .map_err(|e| e.to_string())?
        {
            endpoint_provider.insert(e.id, p.display_name.clone());
        }
    }

    // Library attribution: route remote id -> provider.
    let mut library_provider: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for r in &routes {
        if let Some(pn) = endpoint_provider.get(&r.endpoint_id) {
            library_provider
                .entry(r.remote_model_id.to_lowercase())
                .or_insert_with(|| pn.clone());
        }
    }

    // Catalog attribution (exact remote id across every discovered endpoint).
    let mut catalog_provider: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for p in &providers {
        for e in list_endpoints(&state.pool, p.id)
            .await
            .map_err(|e| e.to_string())?
        {
            for c in list_catalog_models(&state.pool, e.id)
                .await
                .map_err(|e| e.to_string())?
            {
                catalog_provider
                    .entry(c.remote_model_id.to_lowercase())
                    .or_insert_with(|| p.display_name.clone());
            }
        }
    }

    Ok(parsed
        .models
        .iter()
        .map(|m| {
            let remote_lower = m.route.remote_model_id.to_lowercase();
            let match_route = routes.iter().find(|r| {
                r.remote_model_id.to_lowercase() == remote_lower
            });
            let (provider_name, provider_match) =
                if let Some(pn) = library_provider.get(&remote_lower) {
                    (Some(pn.clone()), Some("library".to_string()))
                } else if let Some(pn) = catalog_provider.get(&remote_lower) {
                    (Some(pn.clone()), Some("catalog".to_string()))
                } else {
                    (None, None)
                };
            HarnessModelRow {
                native_id: m.native_id.clone(),
                remote_model_id: m.route.remote_model_id.clone(),
                display_name: m.route.display_name.clone(),
                context_window: m.route.context_window,
                in_library: match_route.is_some(),
                library_route_id: match_route.map(|r| r.id.to_string()),
                library_display_name: match_route.map(|r| r.display_name.clone()),
                provider_name,
                provider_match,
            }
        })
        .collect())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdoptOutcome {
    pub route_id: String,
    pub created: bool,
}

/// Pulls a model configured on the harness (but absent from the library)
/// into My Models under the chosen provider endpoint. Display name and
/// context window come from the harness row. Idempotent: if a route for
/// (endpoint, remote_model_id) already exists it is returned untouched.
#[tauri::command]
pub async fn adopt_harness_model_cmd(
    state: State<'_, AppState>,
    installation_id: String,
    native_id: String,
    endpoint_id: String,
) -> Result<AdoptOutcome, String> {
    let endpoint = Uuid::parse_str(&endpoint_id).map_err(|e| e.to_string())?;
    let (_id, _htype, parsed) = read_parsed_state(&state.pool, &installation_id).await?;
    let model = parsed
        .models
        .iter()
        .find(|m| m.native_id == native_id)
        .ok_or_else(|| format!("model {native_id} not found on this harness"))?;

    let existing = list_routes(&state.pool).await.map_err(|e| e.to_string())?;
    let remote_lower = model.route.remote_model_id.to_lowercase();
    if let Some(already) = existing
        .iter()
        .find(|r| r.endpoint_id == endpoint && r.remote_model_id.to_lowercase() == remote_lower)
    {
        return Ok(AdoptOutcome {
            route_id: already.id.to_string(),
            created: false,
        });
    }

    let mut route = chm_core::domain::models::ModelRoute::new(
        model.route.remote_model_id.clone(),
        model.route.display_name.clone(),
        model.route.context_window,
        serde_json::json!({}),
        serde_json::json!({ "provenance": { "source": "adopted-from-harness" } }),
    );
    route.endpoint_id = endpoint;
    route.max_input = model.route.max_input;
    route.max_output = model.route.max_output;
    let created = create_route(&state.pool, &route)
        .await
        .map_err(|e| e.to_string())?;
    Ok(AdoptOutcome {
        route_id: created.id.to_string(),
        created: true,
    })
}

/// Endpoints grouped by provider for the adopt dialog's dropdown.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointOption {
    pub endpoint_id: String,
    pub provider_name: String,
    pub endpoint_name: String,
    pub protocol: String,
}

#[tauri::command]
pub async fn list_endpoint_options_cmd(
    state: State<'_, AppState>,
) -> Result<Vec<EndpointOption>, String> {
    let providers = list_providers(&state.pool)
        .await
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for p in providers {
        for e in list_endpoints(&state.pool, p.id)
            .await
            .map_err(|e| e.to_string())?
        {
            out.push(EndpointOption {
                endpoint_id: e.id.to_string(),
                provider_name: p.display_name.clone(),
                endpoint_name: e.name,
                protocol: e.protocol.as_str().to_string(),
            });
        }
    }
    out.sort_by(|a, b| a.provider_name.cmp(&b.provider_name));
    Ok(out)
}
// --- Targeted harness model edits (edit / delete / duplicate) ---

use chm_core::domain::history::{ConfigSnapshot, TransactionStatus, TransactionType};
use chm_database::repos::harness::list_installations;
use chm_database::repos::history::{add_snapshot, begin_transaction, finish_transaction};
use chm_harness_sdk::adapter::plan::{ActualState, DesiredState, Mode};
use chm_reconciliation::engine::{filter_unsupported, reconcile};
use serde::Deserialize;
use sha2::Digest;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessModelOp {
    /// "update" | "remove" | "duplicate"
    pub op: String,
    pub native_id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub context_window: Option<i64>,
    #[serde(default)]
    pub remote_model_id: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessEditReport {
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
    pub unchanged: usize,
    pub files_written: Vec<String>,
}

#[tauri::command]
pub async fn apply_harness_model_edits_cmd(
    state: State<'_, AppState>,
    installation_id: String,
    ops: Vec<HarnessModelOp>,
) -> Result<HarnessEditReport, String> {
    if ops.is_empty() {
        return Err("no operations given".into());
    }
    let id = Uuid::parse_str(&installation_id).map_err(|e| e.to_string())?;
    let inst = list_installations(&state.pool)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|i| i.id == id)
        .ok_or_else(|| format!("installation {installation_id} not found"))?;
    let adapter = adapters::all_adapters()
        .into_iter()
        .find(|a| a.id() == inst.harness_type.as_str())
        .ok_or("no adapter for harness")?;
    let parsed = adapter.read_state(&inst).map_err(|e| e.to_string())?;

    // desired = current disk models, modified by the ops. Rows the user did
    // not touch stay byte-identical (Unchanged); a removed row is simply
    // absent from desired, which — combined with its managed flag — makes the
    // reconciler emit a Remove.
    let mut managed: std::collections::HashMap<String, bool> = std::collections::HashMap::new();
    for m in &parsed.models {
        managed.insert(format!("route:{}:{}", m.route.endpoint_id, m.native_id), true);
    }
    for m in &parsed.mcp {
        managed.insert(format!("mcp:{}", m.native_name), false);
    }
    for sk in &parsed.skills {
        managed.insert(format!("skill:{}", sk.path), false);
    }

    let existing_native: std::collections::HashSet<String> =
        parsed.models.iter().map(|m| m.native_id.clone()).collect();

    let mut desired_routes: Vec<chm_core::domain::models::ModelRoute> = Vec::new();
    for m in &parsed.models {
        let op = ops.iter().find(|o| o.native_id == m.native_id);
        match op.map(|o| o.op.as_str()) {
            Some("remove") => {
                // omitted from desired -> Remove
            }
            Some("duplicate") => {
                // keep the original, plus a "-copy" twin
                let mut copy = m.route.clone();
                let mut candidate = format!("{}-copy", m.native_id);
                let mut n = 2;
                while existing_native.contains(&candidate) {
                    candidate = format!("{}-copy-{n}", m.native_id);
                    n += 1;
                }
                copy.remote_model_id = candidate.clone();
                copy.display_name = format!("{} (copy)", m.route.display_name);
                desired_routes.push(copy);
                let mut kept = m.route.clone();
                kept.remote_model_id = m.native_id.clone();
                desired_routes.push(kept);
            }
            Some("update") | None => {
                let mut route = m.route.clone();
                route.remote_model_id = m.native_id.clone();
                if let Some(o) = op {
                    if let Some(dn) = &o.display_name {
                        route.display_name = dn.clone();
                    }
                    if o.context_window.is_some() {
                        route.context_window = o.context_window;
                    }
                    if let Some(rm) = &o.remote_model_id {
                        let rm = rm.trim().to_string();
                        if !rm.is_empty() && rm != m.native_id {
                            // rename: add under the new id; the old native id
                            // drops out of desired -> reconciler removes it.
                            route.remote_model_id = rm;
                        }
                    }
                }
                desired_routes.push(route);
            }
            Some(other) => return Err(format!("unknown op {other}")),
        }
    }

    let desired = DesiredState {
        routes: desired_routes,
        mcp_servers: vec![],
        skills: vec![],
    };
    let actual = ActualState {
        routes: parsed.models.clone(),
        mcp: parsed.mcp.clone(),
        skills: parsed.skills.clone(),
        managed_flags: managed,
    };
    let plan = reconcile(&desired, &actual, Mode::ReplaceManaged).map_err(|e| e.to_string())?;
    let caps = adapter.capabilities();
    let plan = filter_unsupported(plan, &caps);
    let native_plan = adapter.plan(&plan, &inst).map_err(|e| e.to_string())?;

    let mutating = count_kind(&plan, "model", "add")
        + count_kind(&plan, "model", "update")
        + count_kind(&plan, "model", "remove");
    if mutating == 0 {
        return Err("nothing changed by these operations".into());
    }

    let tx = begin_transaction(
        &state.pool,
        TransactionType::Manual,
        serde_json::json!({ "reason": "harness model edit", "plan": native_plan }),
    )
    .await
    .map_err(|e| e.to_string())?;

    // backups first — all-or-nothing before any mutation
    let mut backups: Vec<(String, std::path::PathBuf)> = Vec::new();
    for change in &native_plan.changes {
        match chm_filesystem::backup_file(std::path::Path::new(&change.file_path)) {
            Ok(b) => backups.push((change.file_path.clone(), b)),
            Err(e) => {
                let msg = format!("backup failed before write: {e}");
                let _ = finish_transaction(
                    &state.pool,
                    tx.id,
                    TransactionStatus::Failed,
                    None,
                    Some(msg.clone()),
                )
                .await;
                return Err(msg);
            }
        }
    }

    let apply_outcome = adapter.apply(&inst, &native_plan).map_err(|e| e.to_string());
    match apply_outcome {
        Ok(apply_result) => {
            for (file, backup) in &backups {
                let before = std::fs::read_to_string(backup).ok();
                let after = std::fs::read_to_string(file).ok();
                let hash = |s: &str| format!("{:x}", sha2::Sha256::digest(s.as_bytes()));
                add_snapshot(
                    &state.pool,
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
            }
            let validation = adapter.validate(&inst).map_err(|e| e.to_string())?;
            if validation.ok {
                finish_transaction(
                    &state.pool,
                    tx.id,
                    TransactionStatus::Succeeded,
                    Some(format!(
                        "edited models on {}: +{} ~{} -{}",
                        inst.harness_type.as_str(),
                        count_kind(&plan, "model", "add"),
                        count_kind(&plan, "model", "update"),
                        count_kind(&plan, "model", "remove"),
                    )),
                    None,
                )
                .await
                .map_err(|e| e.to_string())?;
                Ok(HarnessEditReport {
                    files_written: apply_result.files_written,
                    added: count_kind(&plan, "model", "add"),
                    updated: count_kind(&plan, "model", "update"),
                    removed: count_kind(&plan, "model", "remove"),
                    unchanged: count_kind(&plan, "model", "unchanged"),
                })
            } else {
                let msg = format!("validation failed: {:?}", validation.errors);
                for (file, backup) in &backups {
                    let _ = chm_filesystem::restore_backup(
                        std::path::Path::new(file),
                        std::path::Path::new(backup),
                    );
                }
                let _ = finish_transaction(
                    &state.pool,
                    tx.id,
                    TransactionStatus::Failed,
                    None,
                    Some(msg.clone()),
                )
                .await;
                Err(msg)
            }
        }
        Err(e) => {
            for (file, backup) in &backups {
                let _ = chm_filesystem::restore_backup(
                    std::path::Path::new(file),
                    std::path::Path::new(backup),
                );
            }
            let _ = finish_transaction(
                &state.pool,
                tx.id,
                TransactionStatus::Failed,
                None,
                Some(e.clone()),
            )
            .await;
            Err(e)
        }
    }
}

fn count_kind(
    plan: &chm_harness_sdk::adapter::plan::ReconciliationPlan,
    kind: &str,
    action: &str,
) -> usize {
    use chm_harness_sdk::adapter::plan::PlanAction;
    plan.actions
        .iter()
        .filter(|a| match a {
            PlanAction::Add(x) => x.kind == kind && action == "add",
            PlanAction::Update(x) => x.kind == kind && action == "update",
            PlanAction::Remove(x) => x.kind == kind && action == "remove",
            PlanAction::Unchanged(x) => x.kind == kind && action == "unchanged",
            _ => false,
        })
        .count()
}
