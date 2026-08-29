//! Harness-detail model view: disk rows enriched with library linkage, plus
//! adoption of on-device-only models into the library.

use chm_database::repos::models::{create_route, list_catalog_models, list_routes};
use chm_database::repos::providers::{list_endpoints, list_providers};
use serde::Serialize;
use tauri::State;
use uuid::Uuid;

use crate::AppState;
use crate::commands::import::read_parsed_state;

/// Provider grouping declared by the harness config itself, e.g. Pi's
/// models.json: providers.<name>.models[] with `id` fields. Returns
/// (model id -> provider name, provider base url).
fn harness_provider_map(
    inst: &chm_core::domain::harness::HarnessInstallation,
) -> std::collections::HashMap<String, (String, Option<String>)> {
    let mut map = std::collections::HashMap::new();
    let Some(path) = &inst.config_path else {
        return map;
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return map;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return map;
    };
    let Some(providers) = v.get("providers").and_then(|p| p.as_object()) else {
        return map;
    };
    for (pname, p) in providers {
        let base = p
            .get("baseUrl")
            .or_else(|| p.get("base_url"))
            .or_else(|| p.get("url"))
            .and_then(|b| b.as_str())
            .map(|s| s.to_string());
        let models = match p.get("models") {
            Some(serde_json::Value::Array(arr)) => arr.clone(),
            Some(serde_json::Value::Object(obj)) => obj
                .into_iter()
                .map(|(k, v)| {
                    let mut m = serde_json::Map::new();
                    m.insert("id".into(), serde_json::Value::String(k.clone()));
                    if let Some(n) = v.get("name") {
                        m.insert("name".into(), n.clone());
                    }
                    serde_json::Value::Object(m)
                })
                .collect(),
            _ => continue,
        };
        for m in models {
            let mid = m
                .get("id")
                .and_then(|i| i.as_str())
                .map(|s| s.to_lowercase());
            if let Some(mid) = mid {
                map.entry(mid)
                    .or_insert_with(|| (pname.clone(), base.clone()));
            }
        }
    }
    map
}


#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessModelRow {
    pub native_id: String,
    pub native_provider_id: Option<String>,
    pub remote_model_id: String,
    pub display_name: String,
    pub context_window: Option<i64>,
    pub in_library: bool,
    pub library_route_id: Option<String>,
    pub library_display_name: Option<String>,
    /// Provider serving this model, when we can attribute it.
    pub provider_name: Option<String>,
    /// How the provider was attributed. Best first: "harness" (the harness
    /// config itself groups models under a provider), then "library"
    /// (routed My Model), then "catalog" (exact id in a discovered catalog),
    /// with "-suffix" variants for namespaced ids like `gl/glm-5.2`.
    pub provider_match: Option<String>,
    /// Provider base URL when the attribution came from the harness config.
    pub provider_base_url: Option<String>,
    /// Registry provider id, when the attributed provider exists in the
    /// Providers section (drives the provider-detail link).
    pub provider_id: Option<String>,
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
    let inst_for_map = list_installations(&state.pool)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|i| i.id.to_string() == installation_id);
    let native_providers = inst_for_map
        .as_ref()
        .map(harness_provider_map)
        .unwrap_or_default();

    // endpoint -> (provider id, provider display name); base_url -> provider id.
    let mut endpoint_provider: std::collections::HashMap<uuid::Uuid, (uuid::Uuid, String)> =
        std::collections::HashMap::new();
    let mut base_url_provider: std::collections::HashMap<String, uuid::Uuid> =
        std::collections::HashMap::new();
    for p in &providers {
        for e in list_endpoints(&state.pool, p.id)
            .await
            .map_err(|e| e.to_string())?
        {
            endpoint_provider.insert(e.id, (p.id, p.display_name.clone()));
            base_url_provider
                .entry(normalize_base(&e.base_url))
                .or_insert(p.id);
        }
    }

    // Library attribution: route remote id -> (provider id, name).
    let mut library_provider: std::collections::HashMap<String, (uuid::Uuid, String)> =
        std::collections::HashMap::new();
    for r in &routes {
        if let Some(pidpn) = endpoint_provider.get(&r.endpoint_id) {
            library_provider
                .entry(r.remote_model_id.to_lowercase())
                .or_insert(pidpn.clone());
        }
    }

    // Catalog attribution (remote id across every discovered endpoint).
    let mut catalog_provider: std::collections::HashMap<String, (uuid::Uuid, String)> =
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
                    .or_insert((p.id, p.display_name.clone()));
            }
        }
    }

    /// Lookup keys for a harness model id: harnesses commonly prefix gateway
    /// or vendor namespaces onto bare model ids (`gl/glm-5.2`,
    /// `cp/cline-pass/deepseek-v4-flash`), so after the exact id we try each
    /// tail after successive slashes before giving up.
    fn attribution_keys(remote_id: &str) -> Vec<String> {
        let lower = remote_id.to_lowercase();
        let mut keys = vec![lower.clone()];
        let mut rest = lower.as_str();
        while let Some(idx) = rest.find('/') {
            rest = &rest[idx + 1..];
            if rest.is_empty() {
                break;
            }
            keys.push(rest.to_string());
        }
        keys
    }

    Ok(parsed
        .models
        .iter()
        .map(|m| {
            let remote_lower = m.route.remote_model_id.to_lowercase();
            let match_route = routes.iter().find(|r| {
                r.remote_model_id.to_lowercase() == remote_lower
            });
            let keys = attribution_keys(&m.route.remote_model_id);
            let find_in = |map: &std::collections::HashMap<String, (uuid::Uuid, String)>| {
                keys.iter().find_map(|k| map.get(k).cloned())
            };
            let (provider_name, provider_match, provider_base_url, provider_id) = {
                // 1) The harness's own provider grouping is authoritative.
                let native_lower = m.native_id.to_lowercase();
                let native = keys
                    .first()
                    .and_then(|k| native_providers.get(k))
                    .or_else(|| native_providers.get(&native_lower))
                    .cloned();
                if let Some((pname, base)) = native {
                    let pid = base
                        .as_deref()
                        .and_then(|b| base_url_provider.get(&normalize_base(b)))
                        .copied();
                    (
                        Some(pname),
                        Some("harness".to_string()),
                        base,
                        pid.map(|p| p.to_string()),
                    )
                } else if let Some((pid, pn)) = keys
                    .first()
                    .and_then(|k| library_provider.get(k).cloned())
                {
                    (Some(pn), Some("library".to_string()), None, Some(pid.to_string()))
                } else if let Some((pid, pn)) = keys
                    .first()
                    .and_then(|k| catalog_provider.get(k).cloned())
                {
                    (Some(pn), Some("catalog".to_string()), None, Some(pid.to_string()))
                } else if let Some((pid, pn)) = find_in(&library_provider) {
                    (
                        Some(pn),
                        Some("library-suffix".to_string()),
                        None,
                        Some(pid.to_string()),
                    )
                } else if let Some((pid, pn)) = find_in(&catalog_provider) {
                    (
                        Some(pn),
                        Some("catalog-suffix".to_string()),
                        None,
                        Some(pid.to_string()),
                    )
                } else {
                    (None, None, None, None)
                }
            };
            HarnessModelRow {
                native_id: m.native_id.clone(),
                native_provider_id: m
                    .route
                    .overrides
                    .get("native_provider_id")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                remote_model_id: m.route.remote_model_id.clone(),
                display_name: m.route.display_name.clone(),
                context_window: m.route.context_window,
                in_library: match_route.is_some(),
                library_route_id: match_route.map(|r| r.id.to_string()),
                library_display_name: match_route.map(|r| r.display_name.clone()),
                provider_name,
                provider_match,
                provider_base_url,
                provider_id,
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

/// Shared adopt core: idempotently create a My Model route for the given
/// harness row under the chosen endpoint.
async fn adopt_route(
    pool: &sqlx::Pool<sqlx::Sqlite>,
    endpoint: Uuid,
    remote_model_id: &str,
    display_name: &str,
    context_window: Option<i64>,
    max_input: Option<i64>,
    max_output: Option<i64>,
) -> Result<AdoptOutcome, String> {
    let existing = list_routes(pool).await.map_err(|e| e.to_string())?;
    let remote_lower = remote_model_id.to_lowercase();
    if let Some(already) = existing.iter().find(|r| {
        r.endpoint_id == endpoint && r.remote_model_id.to_lowercase() == remote_lower
    }) {
        return Ok(AdoptOutcome {
            route_id: already.id.to_string(),
            created: false,
        });
    }
    let mut route = chm_core::domain::models::ModelRoute::new(
        remote_model_id.to_string(),
        display_name.to_string(),
        context_window,
        serde_json::json!({}),
        serde_json::json!({ "provenance": { "source": "adopted-from-harness" } }),
    );
    route.endpoint_id = endpoint;
    route.max_input = max_input;
    route.max_output = max_output;
    let created = create_route(pool, &route)
        .await
        .map_err(|e| e.to_string())?;
    Ok(AdoptOutcome {
        route_id: created.id.to_string(),
        created: true,
    })
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
    adopt_route(
        &state.pool,
        endpoint,
        &model.route.remote_model_id,
        &model.route.display_name,
        model.route.context_window,
        model.route.max_input,
        model.route.max_output,
    )
    .await
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
    pub native_provider_id: Option<String>,
    /// Optional destination provider for duplicate. Omitted means preserve
    /// the source provider.
    #[serde(default)]
    pub destination_provider_id: Option<String>,
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

    let model_provider = |m: &chm_harness_sdk::adapter::types::HarnessModel| {
        m.route
            .overrides
            .get("native_provider_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase()
    };
    let identity = |provider: &str, id: &str| format!("{provider}\u{1f}{id}");
    let mut used_ids: std::collections::HashSet<String> = parsed
        .models
        .iter()
        .map(|m| identity(&model_provider(m), &m.native_id.to_lowercase()))
        .collect();
    let mut desired_routes: Vec<chm_core::domain::models::ModelRoute> = Vec::new();
    for m in &parsed.models {
        let provider = model_provider(m);
        let candidates: Vec<_> = ops
            .iter()
            .filter(|o| o.native_id == m.native_id)
            .filter(|o| {
                o.native_provider_id
                    .as_deref()
                    .map(|p| p.eq_ignore_ascii_case(&provider))
                    .unwrap_or(true)
            })
            .collect();
        if candidates.len() > 1 {
            return Err(format!(
                "model {} exists under multiple providers; native_provider_id is required",
                m.native_id
            ));
        }
        let op = candidates.first().copied();
        match op.map(|o| o.op.as_str()) {
            Some("remove") => {
                // omitted from desired -> Remove
            }
            Some("duplicate") => {
                let new_id = op
                    .and_then(|o| o.remote_model_id.as_deref())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("{}-copy", m.native_id));
                let destination_provider = op
                    .and_then(|o| o.destination_provider_id.as_deref())
                    .unwrap_or(&provider)
                    .to_string();
                let new_identity = identity(
                    &destination_provider.to_lowercase(),
                    &new_id.to_lowercase(),
                );
                let source_identity = identity(&provider, &m.native_id.to_lowercase());
                if new_identity != source_identity && used_ids.contains(&new_identity) {
                    return Err(format!(
                        "a model named \"{new_id}\" already exists for provider \"{destination_provider}\" on this harness"
                    ));
                }
                let display = op
                    .and_then(|o| o.display_name.as_deref())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("{} (copy)", m.route.display_name));
                let mut copy = m.route.clone();
                copy.remote_model_id = new_id.clone();
                copy.display_name = display;
                copy.overrides["native_provider_id"] =
                    serde_json::Value::String(destination_provider.clone());
                used_ids.insert(new_identity);
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
                            let renamed_identity = identity(&provider, &rm.to_lowercase());
                            let source_identity = identity(&provider, &m.native_id.to_lowercase());
                            if renamed_identity != source_identity && used_ids.contains(&renamed_identity) {
                                return Err(format!(
                                    "a model named \"{rm}\" already exists for provider \"{provider}\" on this harness"
                                ));
                            }
                            // rename: add under the new id; the old native id
                            // drops out of desired -> reconciler removes it.
                            used_ids.remove(&source_identity);
                            used_ids.insert(renamed_identity);
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
    // The adapter may drop actions it cannot write (e.g. an older writer
    // without removal support). Reporting success while writing nothing is
    // worse than failing loudly with the adapter's own explanation.
    if native_plan.changes.is_empty() {
        let detail = if native_plan.warnings.is_empty() {
            "the adapter produced no writable changes for this operation".to_string()
        } else {
            native_plan.warnings.join("; ")
        };
        return Err(format!(
            "this harness adapter cannot write this change yet: {detail}"
        ));
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartAdoptOutcome {
    pub route_id: String,
    pub route_created: bool,
    pub provider_created: bool,
    pub endpoint_created: bool,
    pub provider_name: String,
    pub endpoint_id: String,
}

fn normalize_base(url: &str) -> String {
    url.trim().trim_end_matches('/').to_lowercase()
}

fn slugify(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let trimmed = s.trim_matches('-').to_lowercase();
    if trimmed.is_empty() {
        "provider".into()
    } else {
        trimmed
    }
}

/// One-click import for models whose harness config declares the serving
/// provider (name + base URL). Reuses an existing endpoint with the same
/// base URL, or creates the provider + endpoint on the fly, then routes the
/// model. Falls back to an error when the harness config has no provider
/// info — the UI then shows the manual endpoint picker instead.
#[tauri::command]
pub async fn smart_adopt_harness_model_cmd(
    state: State<'_, AppState>,
    installation_id: String,
    native_id: String,
) -> Result<SmartAdoptOutcome, String> {
    use chm_core::domain::provider::{AuthType, Protocol, ProviderEndpoint};

    let id = Uuid::parse_str(&installation_id).map_err(|e| e.to_string())?;
    let inst = list_installations(&state.pool)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|i| i.id == id)
        .ok_or_else(|| format!("installation {installation_id} not found"))?;
    let (_rid, _htype, parsed) = read_parsed_state(&state.pool, &installation_id).await?;
    let model = parsed
        .models
        .iter()
        .find(|m| m.native_id == native_id)
        .ok_or_else(|| format!("model {native_id} not found on this harness"))?;

    let provider_map = harness_provider_map(&inst);
    let native_lower = native_id.to_lowercase();
    let Some((provider_name, Some(base_url))) = native_providers_lookup(
        &provider_map,
        &model.route.remote_model_id.to_lowercase(),
        &native_lower,
    ) else {
        return Err(
            "this harness config does not declare a provider for this model; choose an endpoint manually"
                .into(),
        );
    };

    // Existing endpoint with the same base URL?
    let providers = list_providers(&state.pool)
        .await
        .map_err(|e| e.to_string())?;
    let target_base = normalize_base(base_url);
    let mut endpoint: Option<chm_core::domain::provider::ProviderEndpoint> = None;
    let mut provider_created = false;
    let mut endpoint_created = false;
    for p in &providers {
        for e in list_endpoints(&state.pool, p.id)
            .await
            .map_err(|e| e.to_string())?
        {
            if normalize_base(&e.base_url) == target_base {
                endpoint = Some(e);
                break;
            }
        }
        if endpoint.is_some() {
            break;
        }
    }

    let endpoint = if let Some(e) = endpoint {
        e
    } else {
        // Create the provider (reuse by slug when present) and its endpoint.
        provider_created = true;
        endpoint_created = true;
        let slug = slugify(provider_name);
        let provider = match chm_database::repos::providers::list_providers(&state.pool)
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|p| p.name == slug)
        {
            Some(p) => p,
            None => {
                chm_database::repos::providers::create_provider(
                    &state.pool,
                    &slug,
                    provider_name,
                )
                .await
                .map_err(|e| e.to_string())?
            }
        };
        chm_database::repos::providers::create_endpoint(
            &state.pool,
            &ProviderEndpoint {
                id: Uuid::new_v4(),
                provider_id: provider.id,
                name: "API".into(),
                base_url: base_url.clone(),
                protocol: Protocol::parse_str("openai-chat"),
                discovery_path: Some("/v1/models".into()),
                auth_type: AuthType::BearerToken,
                credential_ref: None,
                headers: Default::default(),
                enabled: true,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            },
        )
        .await
        .map_err(|e| e.to_string())?
    };

    let outcome = adopt_route(
        &state.pool,
        endpoint.id,
        &model.route.remote_model_id,
        &model.route.display_name,
        model.route.context_window,
        model.route.max_input,
        model.route.max_output,
    )
    .await?;
    Ok(SmartAdoptOutcome {
        route_id: outcome.route_id,
        route_created: outcome.created,
        provider_created,
        endpoint_created,
        provider_name: provider_name.clone(),
        endpoint_id: endpoint.id.to_string(),
    })
}

fn native_providers_lookup<'m>(
    map: &'m std::collections::HashMap<String, (String, Option<String>)>,
    remote_lower: &str,
    native_lower: &str,
) -> Option<&'m (String, Option<String>)> {
    map.get(remote_lower).or_else(|| map.get(native_lower))
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnsureProviderOutcome {
    pub provider_id: String,
    pub provider_created: bool,
    pub endpoint_created: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessProviderDetail {
    pub installation_id: String,
    pub harness_type: String,
    pub provider_name: String,
    pub base_url: Option<String>,
    pub models: Vec<String>,
    pub attribution_confidence: String,
}

/// Read-only provider detail for a provider that exists in a harness config
/// but has not yet been added to CHM's canonical provider registry. This
/// command deliberately performs no database writes.
#[tauri::command]
pub async fn harness_provider_detail_cmd(
    state: State<'_, AppState>,
    installation_id: String,
    provider_name: String,
) -> Result<HarnessProviderDetail, String> {
    let id = Uuid::parse_str(&installation_id).map_err(|e| e.to_string())?;
    let inst = list_installations(&state.pool)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|i| i.id == id)
        .ok_or_else(|| format!("installation {installation_id} not found"))?;
    let provider_map = harness_provider_map(&inst);
    let base_url = provider_map
        .values()
        .find(|(name, _)| name == &provider_name)
        .and_then(|(_, base)| base.clone());
    if !provider_map.values().any(|(name, _)| name == &provider_name) {
        return Err(format!("provider {provider_name} not declared in this harness config"));
    }
    let (_, _, parsed) = read_parsed_state(&state.pool, &installation_id).await?;
    let models = parsed
        .models
        .iter()
        .filter(|m| {
            provider_map
                .get(&m.route.remote_model_id.to_lowercase())
                .or_else(|| provider_map.get(&m.native_id.to_lowercase()))
                .is_some_and(|(name, _)| name == &provider_name)
        })
        .map(|m| m.route.remote_model_id.clone())
        .collect();
    Ok(HarnessProviderDetail {
        installation_id,
        harness_type: inst.harness_type.as_str().to_string(),
        provider_name,
        base_url,
        models,
        attribution_confidence: "declared by harness config".into(),
    })
}

/// Materialize a harness-declared provider (name + base URL from the
/// harness's own config) into the registry so it has a detail page.
/// Reuses the provider by slug and the endpoint by base URL when present.
#[tauri::command]
pub async fn ensure_provider_from_harness_cmd(
    state: State<'_, AppState>,
    installation_id: String,
    provider_name: String,
) -> Result<EnsureProviderOutcome, String> {
    use chm_core::domain::provider::{AuthType, Protocol, ProviderEndpoint};

    let id = Uuid::parse_str(&installation_id).map_err(|e| e.to_string())?;
    let inst = list_installations(&state.pool)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|i| i.id == id)
        .ok_or_else(|| format!("installation {installation_id} not found"))?;

    let provider_map = harness_provider_map(&inst);
    let base_url = provider_map
        .values()
        .find(|(pname, _)| *pname == provider_name)
        .and_then(|(_, base)| base.clone())
        .ok_or_else(|| {
            format!("provider {provider_name} not declared in this harness's config")
        })?;

    let providers = list_providers(&state.pool)
        .await
        .map_err(|e| e.to_string())?;
    let target_base = normalize_base(&base_url);
    let mut provider_created = false;
    let mut endpoint_created = false;

    let mut endpoint: Option<ProviderEndpoint> = None;
    for p in &providers {
        for e in list_endpoints(&state.pool, p.id)
            .await
            .map_err(|e| e.to_string())?
        {
            if normalize_base(&e.base_url) == target_base {
                endpoint = Some(e);
                break;
            }
        }
        if endpoint.is_some() {
            break;
        }
    }

    let provider = if let Some(e) = &endpoint {
        providers
            .iter()
            .find(|p| p.id == e.provider_id)
            .cloned()
            .ok_or("endpoint without provider")?
    } else {
        provider_created = true;
        endpoint_created = true;
        let slug = slugify(&provider_name);
        match providers.into_iter().find(|p| p.name == slug) {
            Some(p) => {
                provider_created = false;
                p
            }
            None => {
                chm_database::repos::providers::create_provider(
                    &state.pool,
                    &slug,
                    &provider_name,
                )
                .await
                .map_err(|e| e.to_string())?
            }
        }
    };

    let _endpoint_id = if let Some(e) = endpoint {
        e.id
    } else {
        chm_database::repos::providers::create_endpoint(
            &state.pool,
            &ProviderEndpoint {
                id: Uuid::new_v4(),
                provider_id: provider.id,
                name: "API".into(),
                base_url,
                protocol: Protocol::parse_str("openai-chat"),
                discovery_path: Some("/v1/models".into()),
                auth_type: AuthType::BearerToken,
                credential_ref: None,
                headers: Default::default(),
                enabled: true,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            },
        )
        .await
        .map_err(|e| e.to_string())?
        .id
    };

    Ok(EnsureProviderOutcome {
        provider_id: provider.id.to_string(),
        provider_created,
        endpoint_created,
    })
}
