//! Import commands: read native state, write canonical state (never native files).

use adapters::all_adapters;
use chm_core::domain::credentials::{CredentialKind, CredentialRef};
use chm_core::domain::harness::HarnessInstallation;
use chm_core::domain::mcp::{McpServer, ScopeType};
use chm_core::domain::models::ModelRoute;
use chm_core::domain::provider::ProviderEndpoint;
use chm_core::domain::skills::Skill;
use chm_database::repos::harness::list_installations;
use chm_database::repos::mcp::{create_mcp_server, list_mcp_servers};
use chm_database::repos::models::{create_route, upsert_catalog_model};
use chm_database::repos::providers::{
    create_credential_ref, create_endpoint, create_provider, list_endpoints, list_providers,
};
use chm_database::repos::skills::{create_skill, list_skills};
use chm_harness_sdk::adapter::types::ParsedState;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Sqlite};
use tauri::State;
use uuid::Uuid;

use crate::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedStateView {
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

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub providers_created: usize,
    pub models_imported: usize,
    pub mcp_imported: usize,
    pub skills_imported: usize,
    pub duplicates: Vec<String>,
}

fn adapter_for(
    harness_type: &str,
) -> Option<Box<dyn chm_harness_sdk::adapter::types::HarnessAdapter>> {
    all_adapters().into_iter().find(|a| a.id() == harness_type)
}

async fn read_parsed_state(
    pool: &Pool<Sqlite>,
    installation_id: &str,
) -> Result<(HarnessInstallation, ParsedState), String> {
    let inst = list_installations(pool)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|i| i.id.to_string() == installation_id)
        .ok_or_else(|| format!("installation {installation_id} not found"))?;
    let adapter = adapter_for(inst.harness_type.as_str()).ok_or("no adapter for harness")?;
    let state = adapter.read_state(&inst).map_err(|e| e.to_string())?;
    Ok((inst, state))
}

#[tauri::command]
pub async fn read_harness_state(
    state: State<'_, AppState>,
    installation_id: String,
) -> Result<ParsedStateView, String> {
    let (_inst, parsed) = read_parsed_state(&state.pool, &installation_id).await?;
    Ok(ParsedStateView {
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

pub async fn run_import(
    pool: &Pool<Sqlite>,
    installation_id: &str,
    options: &ImportOptions,
) -> Result<ImportReport, String> {
    let (inst, parsed) = read_parsed_state(pool, installation_id).await?;
    let mut report = ImportReport::default();
    let provenance = serde_json::json!({
        "source": inst.harness_type.as_str(),
        "imported_at": Utc::now().to_rfc3339(),
    });

    for pv in &parsed.providers {
        let name = pv
            .get("native_provider_id")
            .and_then(|v| v.as_str())
            .unwrap_or("imported");
        if name.starts_with('_') {
            // internal marker entries (e.g. __schema__, __mcp_imports__) are not providers
            continue;
        }
        let providers = list_providers(pool).await.map_err(|e| e.to_string())?;
        if providers.iter().any(|p| p.name == name) {
            report.duplicates.push(format!("provider:{name}"));
            continue;
        }
        let provider = create_provider(pool, name, name).await.map_err(|e| e.to_string())?;
        report.providers_created += 1;

        let base_url = pv.get("base_url").and_then(|v| v.as_str()).map(String::from);
        let protocol = pv.get("protocol").and_then(|v| v.as_str()).unwrap_or("custom");
        let env_key = pv.get("env_key").and_then(|v| v.as_str());
        let credential_ref: Option<CredentialRef> = match env_key {
            Some(key) => Some(
                create_credential_ref(pool, CredentialKind::Env, key)
                    .await
                    .map_err(|e| e.to_string())?,
            ),
            None => None,
        };
        if let Some(base_url) = base_url {
            let endpoint = ProviderEndpoint {
                id: Uuid::new_v4(),
                provider_id: provider.id,
                name: format!("{name}-imported"),
                base_url,
                protocol: chm_core::domain::provider::Protocol::parse_str(protocol),
                discovery_path: Some("/v1/models".into()),
                auth_type: chm_core::domain::provider::AuthType::BearerToken,
                credential_ref,
                headers: Default::default(),
                enabled: true,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };
            create_endpoint(pool, &endpoint).await.map_err(|e| e.to_string())?;
        }
    }

    if options.import_models {
        for m in &parsed.models {
            let endpoint_id = imported_endpoint_id(pool, &inst).await?;
            let route = ModelRoute {
                id: Uuid::new_v4(),
                endpoint_id,
                model_identity_id: None,
                remote_model_id: m.route.remote_model_id.clone(),
                display_name: m.route.display_name.clone(),
                context_window: m.route.context_window,
                max_input: m.route.max_input,
                max_output: m.route.max_output,
                capabilities: m.route.capabilities.clone(),
                overrides: serde_json::json!({
                    "provenance": provenance.clone(),
                    "native": m.route.overrides,
                }),
                enabled: true,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };
            match create_route(pool, &route).await {
                Ok(_) => {
                    report.models_imported += 1;
                    let now = Utc::now();
                    upsert_catalog_model(
                        pool,
                        &chm_core::domain::models::ProviderCatalogModel {
                            id: Uuid::new_v4(),
                            endpoint_id,
                            remote_model_id: route.remote_model_id.clone(),
                            raw_metadata: serde_json::json!({"source": "harness-import"}),
                            canonical_model_id: None,
                            match_confidence: None,
                            first_seen_at: now,
                            last_seen_at: now,
                            missing_since: None,
                            status: chm_core::domain::models::CatalogStatus::New,
                        },
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                }
                Err(_) => {
                    report.duplicates.push(format!("model:{}", m.route.remote_model_id))
                }
            }
        }
    }

    if options.import_mcp {
        for m in &parsed.mcp {
            let existing = list_mcp_servers(pool).await.map_err(|e| e.to_string())?;
            if existing.iter().any(|s| s.name == m.server.name) {
                report.duplicates.push(format!("mcp:{}", m.server.name));
                continue;
            }
            let server = McpServer {
                id: Uuid::new_v4(),
                name: m.server.name.clone(),
                transport: m.server.transport,
                command: m.server.command.clone(),
                args: m.server.args.clone(),
                url: m.server.url.clone(),
                env: m.server.env.clone(),
                scope_type: ScopeType::Global,
                scope_path: None,
                provenance: provenance.clone(),
                enabled: true,
            };
            create_mcp_server(pool, &server).await.map_err(|e| e.to_string())?;
            report.mcp_imported += 1;
        }
    }

    if options.import_skills {
        for s in &parsed.skills {
            if s.symlinked {
                // symlinked skills are already canonical — binding created in Phase 10
                report.skills_imported += 1;
                continue;
            }
            let existing = list_skills(pool).await.map_err(|e| e.to_string())?;
            if existing.iter().any(|sk| sk.canonical_path == s.path) {
                report.duplicates.push(format!("skill:{}", s.name));
                continue;
            }
            let skill = Skill {
                id: Uuid::new_v4(),
                name: s.name.clone(),
                canonical_path: s.path.clone(),
                source_type: chm_core::domain::skills::SkillSourceType::HarnessImport,
                source_url: None,
                content_hash: None,
                provenance: provenance.clone(),
                enabled: true,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };
            create_skill(pool, &skill).await.map_err(|e| e.to_string())?;
            report.skills_imported += 1;
        }
    }

    Ok(report)
}

/// Endpoint used for imported routes: first endpoint of the first imported
/// provider, else a placeholder "imported" endpoint.
async fn imported_endpoint_id(pool: &Pool<Sqlite>, inst: &HarnessInstallation) -> Result<Uuid, String> {
    let providers = list_providers(pool).await.map_err(|e| e.to_string())?;
    for p in providers {
        let endpoints = list_endpoints(pool, p.id).await.map_err(|e| e.to_string())?;
        if let Some(e) = endpoints.first() {
            return Ok(e.id);
        }
    }
    let provider = create_provider(pool, "imported", "Imported (needs setup)")
        .await
        .map_err(|e| e.to_string())?;
    let endpoint = ProviderEndpoint {
        id: Uuid::new_v4(),
        provider_id: provider.id,
        name: format!("{}-imported", inst.harness_type.as_str()),
        base_url: String::new(),
        protocol: chm_core::domain::provider::Protocol::Custom,
        discovery_path: None,
        auth_type: chm_core::domain::provider::AuthType::None,
        credential_ref: None,
        headers: Default::default(),
        enabled: false,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    create_endpoint(pool, &endpoint).await.map_err(|e| e.to_string())?;
    Ok(endpoint.id)
}

#[tauri::command]
pub async fn import_harness_state(
    state: State<'_, AppState>,
    installation_id: String,
    options: ImportOptions,
) -> Result<ImportReport, String> {
    run_import(&state.pool, &installation_id, &options).await
}