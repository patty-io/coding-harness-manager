//! Import commands: thin orchestration over the import service.

use adapters::all_adapters;
use chm_database::repos::harness::list_installations;
use chm_harness_sdk::adapter::types::ParsedState;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Sqlite};
use tauri::State;
use uuid::Uuid;

use crate::AppState;
use crate::services::import::{ImportReport, run_import};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedStateView {
    pub providers: Vec<serde_json::Value>,
    pub models: Vec<serde_json::Value>,
    pub mcp: Vec<serde_json::Value>,
    pub skills: Vec<serde_json::Value>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportOptions {
    pub import_models: bool,
    pub import_mcp: bool,
    pub import_skills: bool,
}

fn adapter_for(
    harness_type: &str,
) -> Option<Box<dyn chm_harness_sdk::adapter::types::HarnessAdapter>> {
    all_adapters().into_iter().find(|a| a.id() == harness_type)
}

pub async fn read_parsed_state(
    pool: &Pool<Sqlite>,
    installation_id: &str,
) -> Result<(uuid::Uuid, String, ParsedState), String> {
    let inst = list_installations(pool)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|i| i.id.to_string() == installation_id)
        .ok_or_else(|| format!("installation {installation_id} not found"))?;
    let adapter = adapter_for(inst.harness_type.as_str()).ok_or("no adapter for harness")?;
    let state = adapter.read_state(&inst).map_err(|e| e.to_string())?;
    Ok((inst.id, inst.harness_type.as_str().to_string(), state))
}

#[tauri::command]
pub async fn read_harness_state(
    state: State<'_, AppState>,
    installation_id: String,
) -> Result<ParsedStateView, String> {
    let (_id, _htype, parsed) = read_parsed_state(&state.pool, &installation_id).await?;
    Ok(ParsedStateView {
        providers: parsed.providers.clone(),
        models: parsed
            .models
            .iter()
            .map(|m| {
                serde_json::json!({
                    "native_id": m.native_id,
                    "remote_model_id": m.route.remote_model_id,
                    "display_name": m.route.display_name,
                    "context_window": m.route.context_window,
                })
            })
            .collect(),
        mcp: parsed
            .mcp
            .iter()
            .map(|m| {
                serde_json::json!({
                    "native_name": m.native_name,
                    "transport": m.server.transport.as_str(),
                    "command": m.server.command,
                })
            })
            .collect(),
        skills: parsed
            .skills
            .iter()
            .map(|s| serde_json::json!({ "name": s.name, "symlinked": s.symlinked }))
            .collect(),
        warnings: parsed.warnings.clone(),
    })
}

/// Returns the primary config file for a harness, verbatim from disk.
/// The Raw config tab mirrors exactly what is on the machine — no
/// interpretation, no merging.
#[tauri::command]
pub async fn read_harness_raw_config(
    state: State<'_, AppState>,
    installation_id: String,
) -> Result<String, String> {
    let inst = list_installations(&state.pool)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|i| i.id.to_string() == installation_id)
        .ok_or_else(|| format!("installation {installation_id} not found"))?;
    let path = inst
        .config_path
        .as_ref()
        .ok_or("harness has no known config path")?;
    std::fs::read_to_string(path).map_err(|e| format!("failed to read {path}: {e}"))
}

#[tauri::command]
pub async fn import_harness_state(
    state: State<'_, AppState>,
    installation_id: String,
    options: ImportOptions,
) -> Result<ImportReport, String> {
    let (_id, htype, parsed) = read_parsed_state(&state.pool, &installation_id).await?;
    let inst = chm_core::domain::harness::HarnessInstallation {
        id: Uuid::new_v4(),
        harness_type: chm_core::domain::harness::HarnessType::parse_str(&htype),
        executable_path: None,
        version: None,
        config_path: None,
        detected_at: chrono::Utc::now(),
        last_scanned_at: None,
        status: chm_core::domain::harness::InstallationStatus::Installed,
    };
    run_import(
        &state.pool,
        &inst,
        &parsed,
        options.import_models,
        options.import_mcp,
        options.import_skills,
    )
    .await
}
