//! Import commands: thin orchestration over the import service.

use adapters::all_adapters;
use chm_harness_sdk::adapter::types::ParsedState;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Sqlite};
use tauri::State;

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
    let (inst, state) = read_parsed_installation(pool, installation_id).await?;
    Ok((inst.id, inst.harness_type.as_str().to_string(), state))
}

/// Read a persisted installation and its adapter state together. Commands
/// that need both should use this form so they do not look the installation up
/// a second time after `read_parsed_state` has already done so.
pub async fn read_parsed_installation(
    pool: &Pool<Sqlite>,
    installation_id: &str,
) -> Result<(chm_core::domain::harness::HarnessInstallation, ParsedState), String> {
    let inst = crate::commands::find_installation(pool, installation_id).await?;
    let adapter = adapter_for(inst.harness_type.as_str()).ok_or("no adapter for harness")?;
    let state = adapter.read_state(&inst).map_err(|e| e.to_string())?;
    Ok((inst, state))
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
    let inst = crate::commands::find_installation(&state.pool, &installation_id).await?;
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
    // Keep the persisted installation identity and metadata. Reconstructing
    // a synthetic installation here discarded the stable id, executable, and
    // config path that downstream import provenance/ownership may need.
    let (inst, parsed) = read_parsed_installation(&state.pool, &installation_id).await?;
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
