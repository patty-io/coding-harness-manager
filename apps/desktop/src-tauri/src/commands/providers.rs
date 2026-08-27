//! Provider CRUD commands.

use chm_core::domain::provider::Provider;
use chm_database::repos::providers;
use sqlx::{Pool, Sqlite};
use tauri::State;
use uuid::Uuid;

use crate::AppState;

#[tauri::command]
pub async fn create_provider_cmd(
    state: State<'_, AppState>,
    name: String,
    display_name: String,
) -> Result<Provider, String> {
    providers::create_provider(&state.pool, &name, &display_name)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_providers_cmd(state: State<'_, AppState>) -> Result<Vec<Provider>, String> {
    providers::list_providers(&state.pool)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_provider_cmd(
    state: State<'_, AppState>,
    id: String,
    display_name: String,
    enabled: bool,
    notes: Option<String>,
) -> Result<Provider, String> {
    let id = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    providers::update_provider(&state.pool, id, &display_name, enabled, notes)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_provider_cmd(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let id = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    providers::delete_provider(&state.pool, id)
        .await
        .map_err(|e| e.to_string())
}

/// Resolves an endpoint across all providers (shared by health/discovery).
pub async fn find_endpoint(
    pool: &Pool<Sqlite>,
    id: Uuid,
) -> Result<chm_core::domain::provider::ProviderEndpoint, String> {
    let providers = providers::list_providers(pool)
        .await
        .map_err(|e| e.to_string())?;
    for p in &providers {
        for e in providers::list_endpoints(pool, p.id)
            .await
            .map_err(|e| e.to_string())?
        {
            if e.id == id {
                return Ok(e);
            }
        }
    }
    Err("endpoint not found".into())
}
