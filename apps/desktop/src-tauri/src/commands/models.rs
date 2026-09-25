//! My Models commands: route CRUD, discovery import, models.dev enrichment.

use chm_core::domain::models::{ModelIdentity, ModelRoute, ProviderCatalogModel};
use chm_database::repos::models::{
    create_identity, create_route, delete_route, get_identity_by_models_dev_id,
    list_catalog_models, list_routes, update_route,
};
use chm_database::repos::providers::{list_endpoints, list_providers};
use chm_models_dev::{ModelsDevModel, bundled_catalog, match_bundled};
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Sqlite};
use std::collections::HashMap;
use tauri::State;
use uuid::Uuid;

use crate::AppState;

fn catalog_route_overrides(catalog_id: &str, provider_name: Option<&str>) -> serde_json::Value {
    let mut overrides = serde_json::json!({
        "provenance": {"source": "provider_discovery", "catalog_id": catalog_id},
    });
    if let Some(provider) = provider_name
        .map(str::trim)
        .filter(|provider| !provider.is_empty())
    {
        overrides["native_provider_id"] = serde_json::Value::String(provider.to_string());
    }
    overrides
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelRouteView {
    pub id: String,
    pub endpoint_id: String,
    pub provider_name: String,
    pub endpoint_name: String,
    pub remote_model_id: String,
    pub display_name: String,
    pub context_window: Option<i64>,
    pub max_input: Option<i64>,
    pub max_output: Option<i64>,
    pub capabilities: serde_json::Value,
    pub overrides: serde_json::Value,
    pub enabled: bool,
    pub identity_name: Option<String>,
    pub provenance: serde_json::Value,
}

async fn get_route(pool: &Pool<Sqlite>, id: Uuid) -> Result<ModelRoute, String> {
    list_routes(pool)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|r| r.id == id)
        .ok_or_else(|| format!("route {id} not found"))
}

pub async fn list_route_views(pool: &Pool<Sqlite>) -> Result<Vec<ModelRouteView>, String> {
    let routes = list_routes(pool).await.map_err(|e| e.to_string())?;
    let providers = list_providers(pool).await.map_err(|e| e.to_string())?;
    let mut endpoint_map = std::collections::HashMap::new();
    for p in &providers {
        for e in list_endpoints(pool, p.id)
            .await
            .map_err(|e| e.to_string())?
        {
            endpoint_map.insert(e.id, (p.display_name.clone(), e.name.clone()));
        }
    }
    let views = routes
        .into_iter()
        .map(|r| {
            let (provider_name, endpoint_name) = endpoint_map
                .get(&r.endpoint_id)
                .cloned()
                .unwrap_or_default();
            ModelRouteView {
                id: r.id.to_string(),
                endpoint_id: r.endpoint_id.to_string(),
                provider_name,
                endpoint_name,
                remote_model_id: r.remote_model_id.clone(),
                display_name: r.display_name.clone(),
                context_window: r.context_window,
                max_input: r.max_input,
                max_output: r.max_output,
                capabilities: r.capabilities.clone(),
                overrides: r.overrides.clone(),
                enabled: r.enabled,
                identity_name: None, // identity names filled after enrichment (Phase 6.4)
                provenance: r
                    .overrides
                    .get("provenance")
                    .cloned()
                    .unwrap_or(serde_json::json!({"source": "unknown"})),
            }
        })
        .collect();
    Ok(views)
}

#[tauri::command]
pub async fn list_routes_cmd(state: State<'_, AppState>) -> Result<Vec<ModelRouteView>, String> {
    list_route_views(&state.pool).await
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteUpdateInput {
    pub display_name: Option<String>,
    pub context_window: Option<i64>,
    pub max_input: Option<i64>,
    pub max_output: Option<i64>,
    pub enabled: Option<bool>,
    pub capabilities: Option<serde_json::Value>,
    pub overrides: Option<serde_json::Value>,
    /// Extended-thinking support, stored in `capabilities.reasoning`.
    pub reasoning: Option<bool>,
    /// Thinking levels the model exposes, stored in
    /// `capabilities.thinking_levels`. Canonical level names only.
    pub thinking_levels: Option<Vec<String>>,
    /// Input modalities the model accepts, stored in
    /// `capabilities.input_modalities` (canonical CHM vocabulary).
    pub input_modalities: Option<Vec<String>>,
}

/// Canonical thinking levels shared with the harness writers. Pi uses exactly
/// this vocabulary (`off` … `max`); other adapters map onto it.
pub const THINKING_LEVELS: [&str; 7] = ["off", "minimal", "low", "medium", "high", "xhigh", "max"];

/// Merge the granular thinking inputs into a route's capabilities blob without
/// disturbing unrelated capability keys. Declaring levels implies the model
/// supports extended thinking even when `reasoning` was not sent.
pub(crate) fn apply_thinking_capabilities(
    capabilities: &mut serde_json::Value,
    reasoning: Option<bool>,
    thinking_levels: Option<Vec<String>>,
) {
    let levels = thinking_levels.map(|levels| {
        let mut cleaned: Vec<String> = Vec::new();
        for level in levels {
            if THINKING_LEVELS.contains(&level.as_str()) && !cleaned.contains(&level) {
                cleaned.push(level);
            }
        }
        cleaned
    });
    let mut object = capabilities.as_object().cloned().unwrap_or_default();
    match reasoning {
        Some(reasoning) => {
            object.insert("reasoning".into(), serde_json::Value::Bool(reasoning));
        }
        None if levels.as_ref().is_some_and(|levels| !levels.is_empty()) => {
            object.insert("reasoning".into(), serde_json::Value::Bool(true));
        }
        None => {}
    }
    if let Some(levels) = levels {
        object.insert("thinking_levels".into(), serde_json::json!(levels));
    }
    *capabilities = serde_json::Value::Object(object);
}

/// Merge the input-modality declaration into a route's capabilities blob
/// without disturbing unrelated capability keys. Values are canonicalized so
/// adapters can rely on the models.dev vocabulary (`text`, `image`, `audio`,
/// `video`, `pdf`) and `text` is always present.
pub(crate) fn apply_modality_capabilities(
    capabilities: &mut serde_json::Value,
    input_modalities: Option<Vec<String>>,
) {
    let Some(modalities) = input_modalities else {
        return;
    };
    let normalized =
        chm_harness_sdk::adapter::capabilities::canonicalize_input_modalities(modalities);
    let mut object = capabilities.as_object().cloned().unwrap_or_default();
    object.insert(
        "input_modalities".into(),
        serde_json::Value::Array(
            normalized
                .into_iter()
                .map(serde_json::Value::String)
                .collect(),
        ),
    );
    *capabilities = serde_json::Value::Object(object);
}

#[tauri::command]
pub async fn update_route_cmd(
    state: State<'_, AppState>,
    id: String,
    input: RouteUpdateInput,
) -> Result<(), String> {
    let route_id = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let mut route = get_route(&state.pool, route_id).await?;
    if let Some(v) = input.display_name {
        route.display_name = v;
    }
    if let Some(v) = input.context_window {
        route.context_window = Some(v);
    }
    if let Some(v) = input.max_input {
        route.max_input = Some(v);
    }
    if let Some(v) = input.max_output {
        route.max_output = Some(v);
    }
    if let Some(v) = input.enabled {
        route.enabled = v;
    }
    if input.reasoning.is_some()
        || input.thinking_levels.is_some()
        || input.input_modalities.is_some()
    {
        let mut capabilities = route.capabilities.clone();
        apply_thinking_capabilities(&mut capabilities, input.reasoning, input.thinking_levels);
        apply_modality_capabilities(&mut capabilities, input.input_modalities);
        route.capabilities = capabilities;
    }
    if let Some(v) = input.capabilities {
        route.capabilities = v;
    }
    if let Some(v) = input.overrides {
        route.overrides = v;
    }
    update_route(&state.pool, &route)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn delete_route_cmd(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let id = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    delete_route(&state.pool, id)
        .await
        .map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteCreateInput {
    pub endpoint_id: String,
    pub remote_model_id: String,
    pub display_name: Option<String>,
    pub context_window: Option<i64>,
    pub max_input: Option<i64>,
    pub max_output: Option<i64>,
    pub capabilities: Option<serde_json::Value>,
}

#[tauri::command]
pub async fn create_route_cmd(
    state: State<'_, AppState>,
    input: RouteCreateInput,
) -> Result<String, String> {
    let endpoint_id = Uuid::parse_str(&input.endpoint_id).map_err(|e| e.to_string())?;
    let remote_model_id = input.remote_model_id;
    if remote_model_id.trim().is_empty() {
        return Err("remote model id is required".into());
    }
    let display_name = input
        .display_name
        .unwrap_or_else(|| remote_model_id.clone());
    let route = ModelRoute::new(
        remote_model_id,
        display_name,
        input.context_window,
        input.capabilities.unwrap_or_else(|| serde_json::json!({})),
        serde_json::json!({"provenance": {"source": "manual"}}),
    );
    let route = ModelRoute {
        endpoint_id,
        max_input: input.max_input,
        max_output: input.max_output,
        ..route
    };
    create_route(&state.pool, &route)
        .await
        .map_err(|e| e.to_string())?;
    Ok(route.id.to_string())
}

// --- Discovery import (6.2) ---

#[derive(Debug, Clone, Serialize)]
pub struct CatalogView {
    pub id: String,
    pub endpoint_id: String,
    pub provider_name: String,
    pub endpoint_name: String,
    pub remote_model_id: String,
    pub status: String,
    pub match_confidence: Option<u8>,
    pub identity_name: Option<String>,
    /// True when a model route already exists for this model on any of the
    /// provider's endpoints. The "Not imported" tab filters on this.
    pub in_my_models: bool,
}

#[tauri::command]
pub async fn list_catalog_all(state: State<'_, AppState>) -> Result<Vec<CatalogView>, String> {
    let providers = list_providers(&state.pool)
        .await
        .map_err(|e| e.to_string())?;
    // Models belong to the provider, not the endpoint: collapse catalog rows
    // across duplicate-protocol endpoints, keeping the best-priority one.
    const PRIORITY: &[&str] = &[
        "openai-chat",
        "openai-responses",
        "openrouter-openai",
        "anthropic-messages",
        "custom",
    ];
    let proto_rank = |p: &str| PRIORITY.iter().position(|x| *x == p).unwrap_or(99);

    // All routed (provider, remote_model_id) pairs, so a model counts as
    // "in My Models" when it is routed on ANY endpoint of its provider.
    let routes = list_routes(&state.pool).await.map_err(|e| e.to_string())?;
    let mut endpoint_provider: HashMap<uuid::Uuid, uuid::Uuid> = HashMap::new();
    let mut provider_endpoints = Vec::new();
    for p in &providers {
        for e in list_endpoints(&state.pool, p.id)
            .await
            .map_err(|e| e.to_string())?
        {
            endpoint_provider.insert(e.id, p.id);
            provider_endpoints.push((p.id, p.display_name.clone(), e));
        }
    }
    let routed: std::collections::HashSet<(uuid::Uuid, String)> = routes
        .into_iter()
        .filter_map(|r| {
            endpoint_provider
                .get(&r.endpoint_id)
                .map(|pid| (*pid, r.remote_model_id.to_lowercase()))
        })
        .collect();

    // Provider identity is the stable key. Display names are user-editable
    // and need not be unique, so using them here could collapse catalogs from
    // two distinct providers into one row.
    let mut best: HashMap<(Uuid, String), (i32, CatalogView)> = HashMap::new();
    for (provider_id, provider_name, e) in provider_endpoints {
        let rank = proto_rank(e.protocol.as_str()) as i32;
        for m in list_catalog_models(&state.pool, e.id)
            .await
            .map_err(|e| e.to_string())?
        {
            let key = (provider_id, m.remote_model_id.to_lowercase());
            let in_my = routed.contains(&(provider_id, m.remote_model_id.to_lowercase()));
            let view = CatalogView {
                id: m.id.to_string(),
                endpoint_id: e.id.to_string(),
                provider_name: provider_name.clone(),
                endpoint_name: e.name.clone(),
                remote_model_id: m.remote_model_id.clone(),
                status: m.status.as_str().to_string(),
                match_confidence: m.match_confidence,
                identity_name: None,
                in_my_models: in_my,
            };
            match best.get(&key) {
                Some((existing_rank, _)) if *existing_rank <= rank => {}
                _ => {
                    best.insert(key, (rank, view));
                }
            }
        }
    }
    let mut views: Vec<CatalogView> = best.into_values().map(|(_, v)| v).collect();
    views.sort_by(|a, b| {
        a.provider_name
            .cmp(&b.provider_name)
            .then(a.remote_model_id.cmp(&b.remote_model_id))
    });
    Ok(views)
}

pub async fn add_catalog_to_my_models(
    pool: &Pool<Sqlite>,
    catalog_id: &str,
) -> Result<String, String> {
    let id = Uuid::parse_str(catalog_id).map_err(|e| e.to_string())?;
    // find the catalog row (scan all endpoints' catalogs)
    let providers = list_providers(pool).await.map_err(|e| e.to_string())?;
    let mut found: Option<ProviderCatalogModel> = None;
    let mut found_provider_name: Option<String> = None;
    for p in &providers {
        for e in list_endpoints(pool, p.id)
            .await
            .map_err(|e| e.to_string())?
        {
            for m in list_catalog_models(pool, e.id)
                .await
                .map_err(|e| e.to_string())?
            {
                if m.id == id {
                    found = Some(m);
                    found_provider_name = Some(p.name.clone());
                }
            }
        }
    }
    let cat = found.ok_or("catalog model not found")?;
    let existing = list_routes(pool).await.map_err(|e| e.to_string())?;
    if existing
        .iter()
        .any(|r| r.endpoint_id == cat.endpoint_id && r.remote_model_id == cat.remote_model_id)
    {
        return Err("already in My Models".into());
    }
    let route = ModelRoute::new(
        cat.remote_model_id.clone(),
        cat.remote_model_id.clone(),
        cat.raw_metadata.get("context").and_then(|v| v.as_i64()),
        cat.raw_metadata.clone(),
        catalog_route_overrides(catalog_id, found_provider_name.as_deref()),
    );
    let route = ModelRoute {
        endpoint_id: cat.endpoint_id,
        ..route
    };
    create_route(pool, &route)
        .await
        .map_err(|e| e.to_string())?;
    Ok(route.id.to_string())
}

#[tauri::command]
pub async fn add_catalog_batch(
    state: State<'_, AppState>,
    catalog_ids: Vec<String>,
) -> Result<usize, String> {
    let pool = &state.pool;
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let existing = list_routes(pool).await.map_err(|e| e.to_string())?;
    let providers = list_providers(pool).await.map_err(|e| e.to_string())?;
    let mut provider_by_endpoint: HashMap<Uuid, String> = HashMap::new();
    for provider in &providers {
        for endpoint in list_endpoints(pool, provider.id)
            .await
            .map_err(|e| e.to_string())?
        {
            provider_by_endpoint.insert(endpoint.id, provider.name.clone());
        }
    }
    let mut seen: std::collections::HashSet<(Uuid, String)> = existing
        .iter()
        .map(|r| (r.endpoint_id, r.remote_model_id.clone()))
        .collect();
    let mut added = 0;
    for cid in &catalog_ids {
        let id = match Uuid::parse_str(cid) {
            Ok(id) => id,
            Err(_) => continue,
        };
        let cat = match chm_database::repos::models::get_catalog_model(pool, id).await {
            Ok(Some(m)) => m,
            _ => continue,
        };
        if seen.contains(&(cat.endpoint_id, cat.remote_model_id.clone())) {
            continue;
        }
        seen.insert((cat.endpoint_id, cat.remote_model_id.clone()));
        let route = ModelRoute::new(
            cat.remote_model_id.clone(),
            cat.remote_model_id.clone(),
            cat.raw_metadata.get("context").and_then(|v| v.as_i64()),
            cat.raw_metadata.clone(),
            catalog_route_overrides(
                cid,
                provider_by_endpoint.get(&cat.endpoint_id).map(String::as_str),
            ),
        );
        let route = ModelRoute {
            endpoint_id: cat.endpoint_id,
            ..route
        };
        if create_route(&mut *tx, &route).await.is_ok() {
            added += 1;
        }
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(added)
}

// --- models.dev enrichment (6.4) ---

fn has_user_override(route: &ModelRoute, field: &str) -> bool {
    route
        .overrides
        .get(field)
        .and_then(|value| value.get("source"))
        .and_then(|value| value.as_str())
        == Some("user_override")
}

/// Extract the input-modality declaration from a models.dev model entry.
/// Returns `None` when the catalog carries no usable declaration.
fn models_dev_input_modalities(model: &ModelsDevModel) -> Option<Vec<String>> {
    let entries = model
        .modalities
        .get("modalities")?
        .get("input")?
        .as_array()?;
    let declared: Vec<&str> = entries.iter().filter_map(|v| v.as_str()).collect();
    if declared.is_empty() {
        return None;
    }
    Some(chm_harness_sdk::adapter::capabilities::canonicalize_input_modalities(declared))
}

async fn get_or_create_identity(
    pool: &Pool<Sqlite>,
    model: &ModelsDevModel,
) -> Result<ModelIdentity, String> {
    if let Some(identity) = get_identity_by_models_dev_id(pool, &model.id)
        .await
        .map_err(|e| e.to_string())?
    {
        return Ok(identity);
    }
    let identity = ModelIdentity {
        id: Uuid::new_v4(),
        canonical_id: model.id.clone(),
        display_name: model.name.clone(),
        family: model.id.split('/').next().map(String::from),
        models_dev_id: Some(model.id.clone()),
        metadata: serde_json::json!({
            "context": model.context_window,
            "max_output": model.max_output,
        }),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    create_identity(pool, &identity)
        .await
        .map_err(|e| e.to_string())
}

fn apply_catalog_metadata(
    mut route: ModelRoute,
    identity: &ModelIdentity,
    model: &ModelsDevModel,
) -> ModelRoute {
    route.model_identity_id = Some(identity.id);
    // models.dev keeps the whole metadata blob in `ModelsDevModel::modalities`,
    // with the modality declaration one level down at `modalities.input`. Only
    // fill it when the route declares nothing yet, so a hand-entered value (or
    // one inherited from endpoint discovery) is never clobbered.
    if chm_harness_sdk::adapter::capabilities::input_modalities(&route.capabilities).is_none()
        && let Some(modalities) = models_dev_input_modalities(model)
    {
        apply_modality_capabilities(&mut route.capabilities, Some(modalities));
    }
    let context_missing = route.context_window.is_none() && !has_user_override(&route, "context_window");
    let output_missing = route.max_output.is_none() && !has_user_override(&route, "max_output");
    let overrides = route
        .overrides
        .as_object_mut()
        .expect("model route overrides must be a JSON object");
    if context_missing
        && let Some(context) = model.context_window
    {
        route.context_window = Some(context);
        overrides
            .entry("context_window".to_string())
            .or_insert_with(|| serde_json::json!({"value": context, "source": "models.dev"}));
    }
    if output_missing
        && let Some(max_output) = model.max_output
    {
        route.max_output = Some(max_output);
        overrides
            .entry("max_output".to_string())
            .or_insert_with(|| serde_json::json!({"value": max_output, "source": "models.dev"}));
    }
    overrides.insert(
        "model_identity_id".to_string(),
        serde_json::json!({"value": identity.id.to_string(), "source": "models.dev"}),
    );
    route
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrichCandidate {
    pub models_dev_id: String,
    pub display_name: String,
    pub context_window: Option<i64>,
    pub max_output: Option<i64>,
    pub confidence: u8,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EnrichOutcome {
    Matched {
        confidence: u8,
        identity_id: String,
        identity_name: String,
    },
    Ambiguous {
        candidates: Vec<EnrichCandidate>,
        current: serde_json::Value,
    },
    Unknown,
}

pub async fn enrich_route(pool: &Pool<Sqlite>, route_id: &str) -> Result<EnrichOutcome, String> {
    let id = Uuid::parse_str(route_id).map_err(|e| e.to_string())?;
    let route = get_route(pool, id).await?;
    // user override on any field wins — enrichment never overwrites it
    if route
        .overrides
        .get("context_window")
        .and_then(|v| v.get("source"))
        .and_then(|v| v.as_str())
        == Some("user_override")
    {
        return Ok(EnrichOutcome::Unknown);
    }
    let hit = match_bundled(&route.remote_model_id);
    match hit.confidence {
        0 => Ok(EnrichOutcome::Unknown),
        c if c >= 85 => {
            let model = hit.model.expect("confidence implies model");
            let identity = get_or_create_identity(pool, &model).await?;
            let updated = apply_catalog_metadata(route.clone(), &identity, &model);
            update_route(pool, &updated)
                .await
                .map_err(|e| e.to_string())?;
            Ok(EnrichOutcome::Matched {
                confidence: c,
                identity_id: identity.id.to_string(),
                identity_name: identity.display_name,
            })
        }
        _ => {
            // ambiguous: collect candidates at >= 60 confidence
            let family = route
                .remote_model_id
                .rsplit('/')
                .next()
                .unwrap_or(&route.remote_model_id);
            let candidates: Vec<EnrichCandidate> = chm_models_dev::bundled_catalog()
                .models
                .iter()
                .filter(|m| {
                    let norm = |s: &str| {
                        s.to_lowercase()
                            .chars()
                            .filter(|ch| ch.is_ascii_alphanumeric())
                            .collect::<String>()
                    };
                    norm(&m.id).contains(&norm(family)) && norm(&m.id) != norm(family)
                })
                .take(10)
                .map(|m| EnrichCandidate {
                    models_dev_id: m.id.clone(),
                    display_name: m.name.clone(),
                    context_window: m.context_window,
                    max_output: m.max_output,
                    confidence: 60,
                })
                .collect();
            if candidates.is_empty() {
                Ok(EnrichOutcome::Unknown)
            } else {
                Ok(EnrichOutcome::Ambiguous {
                    candidates,
                    current: route.overrides.clone(),
                })
            }
        }
    }
}

#[tauri::command]
pub async fn enrich_route_cmd(
    state: State<'_, AppState>,
    route_id: String,
) -> Result<EnrichOutcome, String> {
    enrich_route(&state.pool, &route_id).await
}

#[tauri::command]
pub async fn resolve_enrichment_cmd(
    state: State<'_, AppState>,
    route_id: String,
    identity_id: String,
) -> Result<(), String> {
    let id = Uuid::parse_str(&route_id).map_err(|e| e.to_string())?;
    let routes = list_routes(&state.pool).await.map_err(|e| e.to_string())?;
    let route = routes
        .iter()
        .find(|r| r.id == id)
        .ok_or("route not found")?;
    let updated = if let Ok(iid) = Uuid::parse_str(&identity_id) {
        // Keep accepting the original identity UUID form for callers that
        // already resolved a candidate.
        let mut updated = route.clone();
        updated.model_identity_id = Some(iid);
        updated
            .overrides
            .as_object_mut()
            .expect("model route overrides must be a JSON object")
            .entry("model_identity_id".to_string())
            .or_insert_with(|| serde_json::json!(identity_id));
        updated
    } else {
        // Ambiguous candidates are represented by their models.dev id in the
        // UI, not by a database UUID. Materialize/reuse the identity here.
        let model = bundled_catalog()
            .models
            .iter()
            .find(|model| model.id == identity_id)
            .ok_or_else(|| format!("unknown models.dev candidate {identity_id}"))?;
        let identity = get_or_create_identity(&state.pool, model).await?;
        apply_catalog_metadata(route.clone(), &identity, model)
    };
    update_route(&state.pool, &updated)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn set_user_override_cmd(
    state: State<'_, AppState>,
    route_id: String,
    field: String,
    value: Option<serde_json::Value>,
) -> Result<(), String> {
    let id = Uuid::parse_str(&route_id).map_err(|e| e.to_string())?;
    let routes = list_routes(&state.pool).await.map_err(|e| e.to_string())?;
    let route = routes
        .iter()
        .find(|r| r.id == id)
        .ok_or("route not found")?;
    let mut overrides = route.overrides.clone();
    overrides.as_object_mut().unwrap().insert(
        field,
        serde_json::json!({"value": value, "source": "user_override"}),
    );
    let updated = ModelRoute {
        overrides,
        ..route.clone()
    };
    update_route(&state.pool, &updated)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{EnrichOutcome, apply_catalog_metadata, apply_modality_capabilities, catalog_route_overrides};
    use chm_core::domain::models::{ModelIdentity, ModelRoute};
    use chm_models_dev::ModelsDevModel;
    use serde_json::json;
    use uuid::Uuid;

    #[test]
    fn discovered_route_keeps_the_canonical_provider_identity() {
        let overrides = catalog_route_overrides("catalog-id", Some("yolo-auto"));

        assert_eq!(
            overrides.get("native_provider_id").and_then(|value| value.as_str()),
            Some("yolo-auto")
        );
        assert_eq!(
            overrides["provenance"]["source"],
            serde_json::json!("provider_discovery")
        );
    }

    #[test]
    fn enrich_outcome_uses_the_camel_case_wire_shape() {
        let matched = serde_json::to_value(EnrichOutcome::Matched {
            confidence: 100,
            identity_id: "identity".into(),
            identity_name: "GLM".into(),
        })
        .unwrap();
        assert_eq!(matched["matched"]["identity_id"], "identity");
        assert!(matched.get("Matched").is_none());

        let unknown = serde_json::to_value(EnrichOutcome::Unknown).unwrap();
        assert_eq!(unknown, json!("unknown"));
    }

    #[test]
    fn catalog_metadata_fills_missing_route_limits_without_overwriting_identity() {
        let route = ModelRoute::new(
            "glm-5".into(),
            "GLM 5".into(),
            None,
            json!({}),
            json!({}),
        );
        let identity = ModelIdentity {
            id: Uuid::new_v4(),
            canonical_id: "zai-org/glm-5".into(),
            display_name: "GLM 5".into(),
            family: Some("zai-org".into()),
            models_dev_id: Some("zai-org/glm-5".into()),
            metadata: json!({}),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let model = ModelsDevModel {
            id: "zai-org/glm-5".into(),
            name: "GLM 5".into(),
            provider: Some("zai".into()),
            context_window: Some(200_000),
            max_output: Some(32_000),
            modalities: json!({}),
        };

        let updated = apply_catalog_metadata(route, &identity, &model);
        assert_eq!(updated.context_window, Some(200_000));
        assert_eq!(updated.max_output, Some(32_000));
        assert_eq!(updated.model_identity_id, Some(identity.id));
        assert_eq!(updated.overrides["context_window"]["source"], "models.dev");
    }

    fn identity() -> ModelIdentity {
        ModelIdentity {
            id: Uuid::new_v4(),
            canonical_id: "zai-org/glm-5".into(),
            display_name: "GLM 5".into(),
            family: Some("zai-org".into()),
            models_dev_id: Some("zai-org/glm-5".into()),
            metadata: json!({}),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    fn model_with_modalities(input: serde_json::Value) -> ModelsDevModel {
        ModelsDevModel {
            id: "zai-org/glm-5".into(),
            name: "GLM 5".into(),
            provider: Some("zai".into()),
            context_window: None,
            max_output: None,
            modalities: json!({"modalities": {"input": input, "output": ["text"]}}),
        }
    }

    #[test]
    fn catalog_enrichment_declares_modalities_when_the_route_is_silent() {
        let route = ModelRoute::new("glm-5".into(), "GLM 5".into(), None, json!({}), json!({}));
        let model = model_with_modalities(json!(["video", "image", "not-a-modality"]));
        let updated = apply_catalog_metadata(route, &identity(), &model);
        assert_eq!(
            updated.capabilities["input_modalities"],
            json!(["text", "image", "video"]),
            "unknown values drop, canonical order applies, and text is always present"
        );
    }

    #[test]
    fn catalog_enrichment_never_overwrites_a_declared_modality_set() {
        let route = ModelRoute::new(
            "glm-5".into(),
            "GLM 5".into(),
            None,
            json!({"input_modalities": ["text"]}),
            json!({}),
        );
        let model = model_with_modalities(json!(["text", "image"]));
        let updated = apply_catalog_metadata(route, &identity(), &model);
        assert_eq!(
            updated.capabilities["input_modalities"],
            json!(["text"]),
            "a hand-entered (or discovery-inherited) declaration wins over the catalog"
        );
    }

    #[test]
    fn modality_updates_keep_unrelated_capability_keys() {
        let mut capabilities = json!({"reasoning": true, "thinking_levels": ["high"]});
        apply_modality_capabilities(
            &mut capabilities,
            Some(vec!["image".to_string(), "bogus".to_string()]),
        );
        assert_eq!(capabilities["reasoning"], json!(true));
        assert_eq!(capabilities["thinking_levels"], json!(["high"]));
        assert_eq!(capabilities["input_modalities"], json!(["text", "image"]));

        // `None` means "caller sent nothing" and must be a no-op.
        apply_modality_capabilities(&mut capabilities, None);
        assert_eq!(capabilities["input_modalities"], json!(["text", "image"]));
    }
}
