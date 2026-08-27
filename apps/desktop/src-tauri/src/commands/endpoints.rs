//! Endpoint + credential commands.

use chm_core::domain::credentials::CredentialKind;
use chm_core::domain::provider::{AuthType, Protocol, ProviderEndpoint};
use chm_database::repos::providers::{create_credential_ref, create_endpoint, list_endpoints};
use serde::Deserialize;
use sqlx::{Pool, Sqlite};
use tauri::State;
use uuid::Uuid;

use crate::AppState;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointInput {
    pub provider_id: String,
    pub name: String,
    pub base_url: String,
    pub protocol: String,
    pub discovery_path: Option<String>,
    pub auth_type: String,
    pub credential_ref_id: Option<String>,
    pub headers: serde_json::Map<String, serde_json::Value>,
    pub enabled: bool,
}

#[tauri::command]
pub async fn list_endpoints_cmd(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<Vec<ProviderEndpoint>, String> {
    let provider_id = Uuid::parse_str(&provider_id).map_err(|e| e.to_string())?;
    list_endpoints(&state.pool, provider_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_endpoint_cmd(
    state: State<'_, AppState>,
    input: EndpointInput,
) -> Result<ProviderEndpoint, String> {
    let provider_id = Uuid::parse_str(&input.provider_id).map_err(|e| e.to_string())?;
    let credential_ref = match &input.credential_ref_id {
        Some(cid) => Some(
            chm_database::repos::providers::get_credential_ref(
                &state.pool,
                Uuid::parse_str(cid).map_err(|e| e.to_string())?,
            )
            .await
            .map_err(|e| e.to_string())?,
        ),
        None => None,
    };
    let endpoint = ProviderEndpoint {
        id: Uuid::new_v4(),
        provider_id,
        name: input.name,
        base_url: input.base_url,
        protocol: Protocol::parse_str(&input.protocol),
        discovery_path: input.discovery_path,
        auth_type: AuthType::parse_str(&input.auth_type),
        credential_ref,
        headers: input.headers,
        enabled: input.enabled,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    create_endpoint(&state.pool, &endpoint)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn save_api_key(
    state: State<'_, AppState>,
    key_name: String,
    value: String,
) -> Result<String, String> {
    let store_key = format!("providers/{key_name}");
    state
        .secrets
        .set(&store_key, &value)
        .map_err(|e| e.to_string())?;
    let reference = format!("coding-harness-manager/{store_key}");
    let cred = create_credential_ref(&state.pool, CredentialKind::Keychain, &reference)
        .await
        .map_err(|e| e.to_string())?;
    Ok(cred.id.to_string())
}

#[tauri::command]
pub async fn env_var_set(state: State<'_, AppState>, var_name: String) -> Result<bool, String> {
    Ok(state
        .secrets
        .get(&var_name)
        .map_err(|e| e.to_string())?
        .is_some())
}

/// Shared: resolve an endpoint's resolved credential (env or keychain).
pub fn resolve_endpoint_credential(
    endpoint: &ProviderEndpoint,
    secrets: &dyn chm_secrets::SecretStore,
) -> Option<String> {
    endpoint
        .credential_ref
        .as_ref()
        .and_then(|c| chm_providers::resolve_credential(c, secrets))
}

#[allow(dead_code)]
fn _pool_marker(_: &Pool<Sqlite>) {}
