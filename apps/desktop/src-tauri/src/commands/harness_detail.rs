//! Harness-detail model view: disk rows enriched with library linkage, plus
//! adoption of on-device-only models into the library.

use chm_database::repos::models::{create_route, list_routes};
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
}

#[tauri::command]
pub async fn harness_models_view_cmd(
    state: State<'_, AppState>,
    installation_id: String,
) -> Result<Vec<HarnessModelRow>, String> {
    let (_id, _htype, parsed) = read_parsed_state(&state.pool, &installation_id).await?;
    let routes = list_routes(&state.pool).await.map_err(|e| e.to_string())?;

    Ok(parsed
        .models
        .iter()
        .map(|m| {
            let remote_lower = m.route.remote_model_id.to_lowercase();
            let match_route = routes.iter().find(|r| {
                r.remote_model_id.to_lowercase() == remote_lower
            });
            HarnessModelRow {
                native_id: m.native_id.clone(),
                remote_model_id: m.route.remote_model_id.clone(),
                display_name: m.route.display_name.clone(),
                context_window: m.route.context_window,
                in_library: match_route.is_some(),
                library_route_id: match_route.map(|r| r.id.to_string()),
                library_display_name: match_route.map(|r| r.display_name.clone()),
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