//! Provider health-check + model discovery commands (Phase 5.4).

use chm_core::domain::models::ProviderCatalogModel;
use chm_database::repos::models::{list_catalog_models, upsert_catalog_model};
use chm_providers::{discover_models, health_check};
use serde::Serialize;
use tauri::State;
use uuid::Uuid;

use crate::AppState;
use crate::commands::endpoints::resolve_endpoint_credential;
use crate::commands::providers::find_endpoint;

#[tauri::command]
pub async fn check_endpoint_health(
    state: State<'_, AppState>,
    endpoint_id: String,
) -> Result<String, String> {
    let id = Uuid::parse_str(&endpoint_id).map_err(|e| e.to_string())?;
    let endpoint = find_endpoint(&state.pool, id).await?;
    let cred = resolve_endpoint_credential(&endpoint, state.secrets.as_ref());
    let status = health_check(&endpoint, cred.as_deref(), &state.http).await;
    Ok(format!("{status:?}"))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverReport {
    pub total: usize,
    pub added: usize,
    pub updated: usize,
}

#[tauri::command]
pub async fn discover_endpoint_models(
    state: State<'_, AppState>,
    endpoint_id: String,
) -> Result<DiscoverReport, String> {
    let id = Uuid::parse_str(&endpoint_id).map_err(|e| e.to_string())?;
    let endpoint = find_endpoint(&state.pool, id).await?;
    let cred = resolve_endpoint_credential(&endpoint, state.secrets.as_ref());
    let models = discover_models(&endpoint, cred.as_deref(), &state.http)
        .await
        .map_err(|e| e.to_string())?;
    let existing = list_catalog_models(&state.pool, id)
        .await
        .map_err(|e| e.to_string())?;
    let mut added = 0;
    let mut updated = 0;
    let now = chrono::Utc::now();
    for m in &models {
        let is_new = !existing.iter().any(|c| c.remote_model_id == m.id);
        upsert_catalog_model(
            &state.pool,
            &ProviderCatalogModel {
                id: Uuid::new_v4(),
                endpoint_id: id,
                remote_model_id: m.id.clone(),
                raw_metadata: m.raw.clone(),
                canonical_model_id: None,
                match_confidence: None,
                first_seen_at: now,
                last_seen_at: now,
                missing_since: None,
                status: if is_new {
                    chm_core::domain::models::CatalogStatus::New
                } else {
                    chm_core::domain::models::CatalogStatus::Available
                },
            },
        )
        .await
        .map_err(|e| e.to_string())?;
        if is_new {
            added += 1;
        } else {
            updated += 1;
        }
    }
    Ok(DiscoverReport {
        total: models.len(),
        added,
        updated,
    })
}

#[tauri::command]
pub async fn list_catalog_models_cmd(
    state: State<'_, AppState>,
    endpoint_id: String,
) -> Result<Vec<ProviderCatalogModel>, String> {
    let id = Uuid::parse_str(&endpoint_id).map_err(|e| e.to_string())?;
    list_catalog_models(&state.pool, id)
        .await
        .map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSummary {
    pub endpoints: usize,
    pub discovered_models: usize,
    pub my_models: usize,
    pub health: String,
}

#[tauri::command]
pub async fn provider_summary(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<ProviderSummary, String> {
    let id = Uuid::parse_str(&provider_id).map_err(|e| e.to_string())?;
    let endpoints = chm_database::repos::providers::list_endpoints(&state.pool, id)
        .await
        .map_err(|e| e.to_string())?;
    let mut discovered = 0;
    for e in &endpoints {
        discovered += list_catalog_models(&state.pool, e.id)
            .await
            .map_err(|e| e.to_string())?
            .len();
    }
    let endpoint_ids: std::collections::HashSet<Uuid> = endpoints.iter().map(|e| e.id).collect();
    let routes = chm_database::repos::models::list_routes(&state.pool)
        .await
        .map_err(|e| e.to_string())?;
    let my_models = routes
        .iter()
        .filter(|r| endpoint_ids.contains(&r.endpoint_id))
        .count();
    Ok(ProviderSummary {
        endpoints: endpoints.len(),
        discovered_models: discovered,
        my_models,
        health: "unknown".into(),
    })
}
