//! My Models commands: route CRUD, discovery import, models.dev enrichment.

use chm_core::domain::models::{CatalogStatus, ModelIdentity, ModelRoute, ProviderCatalogModel};
use chm_database::repos::models::{
    create_identity, create_route, delete_route, list_catalog_models, list_routes, update_route,
};
use chm_database::repos::providers::{list_endpoints, list_providers};
use chm_models_dev::{bundled_catalog, match_model};
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Sqlite};
use tauri::State;
use uuid::Uuid;

use crate::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct ModelRouteView {
    pub id: String,
    pub endpoint_id: String,
    pub provider_name: String,
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

/// Applies a provenance envelope's value into the matching route field.
fn apply_override_to_field(route: &mut ModelRoute, field: &str, value: &serde_json::Value) {
    match field {
        "context_window" => route.context_window = value.as_i64(),
        "max_input" => route.max_input = value.as_i64(),
        "max_output" => route.max_output = value.as_i64(),
        _ => {}
    }
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
            let (provider_name, _endpoint_name) = endpoint_map
                .get(&r.endpoint_id)
                .cloned()
                .unwrap_or_default();
            ModelRouteView {
                id: r.id.to_string(),
                endpoint_id: r.endpoint_id.to_string(),
                provider_name,
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
}

#[tauri::command]
pub async fn list_catalog_all(state: State<'_, AppState>) -> Result<Vec<CatalogView>, String> {
    let providers = list_providers(&state.pool)
        .await
        .map_err(|e| e.to_string())?;
    let mut views = Vec::new();
    for p in &providers {
        for e in list_endpoints(&state.pool, p.id)
            .await
            .map_err(|e| e.to_string())?
        {
            for m in list_catalog_models(&state.pool, e.id)
                .await
                .map_err(|e| e.to_string())?
            {
                views.push(CatalogView {
                    id: m.id.to_string(),
                    endpoint_id: e.id.to_string(),
                    provider_name: p.display_name.clone(),
                    endpoint_name: e.name.clone(),
                    remote_model_id: m.remote_model_id.clone(),
                    status: m.status.as_str().to_string(),
                    match_confidence: m.match_confidence,
                    identity_name: None,
                });
            }
        }
    }
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
        serde_json::json!({
            "provenance": {"source": "provider_discovery", "catalog_id": catalog_id},
        }),
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
            serde_json::json!({
                "provenance": {"source": "provider_discovery", "catalog_id": cid},
            }),
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
    let catalog = bundled_catalog();
    let hit = match_bundled(&route.remote_model_id);
    match hit.confidence {
        0 => Ok(EnrichOutcome::Unknown),
        c if c >= 85 => {
            let model = hit.model.expect("confidence implies model");
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
                .map_err(|e| e.to_string())?;
            // write field provenance (models.dev source) — only where the user
            // hasn't overridden and no value exists yet
            let mut overrides = route.overrides.clone();
            let fields = overrides.as_object_mut().unwrap();
            fields
                .entry("context_window".to_string())
                .or_insert_with(|| {
                    serde_json::json!({
                        "value": model.context_window,
                        "source": "models.dev",
                    })
                });
            let updated = ModelRoute {
                model_identity_id: Some(identity.id),
                overrides,
                ..route.clone()
            };
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
            let candidates: Vec<EnrichCandidate> = catalog
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
    let iid = Uuid::parse_str(&identity_id).map_err(|e| e.to_string())?;
    let routes = list_routes(&state.pool).await.map_err(|e| e.to_string())?;
    let route = routes
        .iter()
        .find(|r| r.id == id)
        .ok_or("route not found")?;
    let mut overrides = route.overrides.clone();
    overrides
        .as_object_mut()
        .unwrap()
        .entry("model_identity_id".to_string())
        .or_insert_with(|| serde_json::json!(identity_id));
    let updated = ModelRoute {
        model_identity_id: Some(iid),
        overrides,
        ..route.clone()
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
