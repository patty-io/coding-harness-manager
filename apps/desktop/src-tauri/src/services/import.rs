//! Harness-import service: pure canonicalization + atomic apply.
//! Extracted from the import command (team-protocol batch-1) so Phase 5/6
//! flows reuse the same mechanics.

use chm_core::domain::credentials::{CredentialKind, CredentialRef};
use chm_core::domain::harness::HarnessInstallation;
use chm_core::domain::mcp::{McpServer, ScopeType};
use chm_core::domain::models::ModelRoute;
use chm_core::domain::provider::{AuthType, Protocol, ProviderEndpoint};
use chm_core::domain::skills::Skill;
use chm_database::DbError;
use chm_database::repos::mcp::{create_mcp_server, list_mcp_servers};
use chm_database::repos::models::{create_route, upsert_catalog_model};
use chm_database::repos::providers::{
    create_credential_ref, create_endpoint, create_provider, list_endpoints, list_providers,
};
use chm_database::repos::skills::{create_skill, list_skills};
use chm_harness_sdk::adapter::types::ParsedState;
use chrono::Utc;
use serde::Serialize;
use sqlx::{Pool, Sqlite};
use uuid::Uuid;

/// What an import plans to do with each entity kind, decided BEFORE any write.
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub providers_created: usize,
    pub models_imported: usize,
    pub mcp_imported: usize,
    pub skills_imported: usize,
    pub skills_symlinked: usize,
    pub duplicates: Vec<String>,
}

/// Maps native protocol hints to the domain Protocol:
/// opencode payload: `protocol`; pi: `api` ("openai-completions"); reasonix:
/// `kind` ("openai"/"anthropic"); codex: `wire_api` ("chat"/"responses").
pub fn protocol_from_native(
    protocol: Option<&str>,
    api: Option<&str>,
    kind: Option<&str>,
    wire_api: Option<&str>,
) -> Protocol {
    let s = protocol.or(api).or(kind).or(wire_api).unwrap_or("");
    match s {
        "anthropic" | "anthropic-messages" => Protocol::AnthropicMessages,
        "openai-chat" | "openai-completions" | "chat" => Protocol::OpenAiChatCompletions,
        "openai-responses" | "responses" => Protocol::OpenAiResponses,
        "openrouter-openai" | "openrouter" => Protocol::OpenRouterOpenAi,
        _ => Protocol::Custom,
    }
}

pub fn is_unique_violation(e: &DbError) -> bool {
    matches!(
        e,
        DbError::Sqlx(sqlx::Error::Database(db)) if db.is_unique_violation()
    )
}

/// Applies one harness's parsed state to the canonical registry, atomically.
/// Dedup policy lives here: name conflicts are skipped and reported, never
/// overwritten; only unique-constraint hits count as duplicates; any other
/// failure rolls back the whole import.
pub async fn run_import(
    pool: &Pool<Sqlite>,
    inst: &HarnessInstallation,
    parsed: &ParsedState,
    import_models: bool,
    import_mcp: bool,
    import_skills: bool,
) -> Result<ImportReport, String> {
    let mut report = ImportReport::default();
    let provenance = serde_json::json!({
        "source": inst.harness_type.as_str(),
        "imported_at": Utc::now().to_rfc3339(),
    });

    // Hoist existence checks once — never re-query inside loops.
    let existing_providers = list_providers(pool).await.map_err(|e| e.to_string())?;
    let existing_mcp = list_mcp_servers(pool).await.map_err(|e| e.to_string())?;
    let existing_skills = list_skills(pool).await.map_err(|e| e.to_string())?;
    let mut endpoint_by_native_provider: std::collections::HashMap<String, Uuid> =
        std::collections::HashMap::new();
    // pre-seed from already-registered providers so a re-import links routes
    // to the SAME endpoint instead of minting junk placeholder ones
    for p in &existing_providers {
        if let Ok(endpoints) = list_endpoints(pool, p.id).await
            && let Some(e) = endpoints.first()
        {
            endpoint_by_native_provider.insert(p.name.clone(), e.id);
        }
    }
    let mut created_in_batch: std::collections::HashSet<String> = Default::default();
    // track names/paths seen within THIS batch: duplicates are reported, not fatal
    let mut batch_mcp: std::collections::HashSet<String> = Default::default();
    let mut batch_skills: std::collections::HashSet<String> = Default::default();

    // The whole import is one transaction: any failure rolls back everything.
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    for pv in &parsed.providers {
        let name = pv
            .get("native_provider_id")
            .and_then(|v| v.as_str())
            .unwrap_or("imported");
        if name.starts_with('_') {
            continue; // internal marker entries (__schema__, __mcp_imports__)
        }
        if existing_providers.iter().any(|p| p.name == name) || created_in_batch.contains(name) {
            report.duplicates.push(format!("provider:{name}"));
            continue;
        }
        let provider = create_provider(&mut *tx, name, name)
            .await
            .map_err(|e| e.to_string())?;
        created_in_batch.insert(name.to_string());
        report.providers_created += 1;

        let base_url = pv
            .get("base_url")
            .and_then(|v| v.as_str())
            .map(String::from);
        let env_key = pv
            .get("env_key")
            .or_else(|| pv.get("env_reference"))
            .and_then(|v| v.as_str());
        let credential_ref: Option<CredentialRef> = match env_key {
            Some(key) => Some(
                create_credential_ref(&mut *tx, CredentialKind::Env, key)
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
                protocol: protocol_from_native(
                    pv.get("protocol").and_then(|v| v.as_str()),
                    pv.get("api").and_then(|v| v.as_str()),
                    pv.get("kind").and_then(|v| v.as_str()),
                    pv.get("wire_api").and_then(|v| v.as_str()),
                ),
                discovery_path: Some("/v1/models".into()),
                auth_type: if credential_ref.is_some() {
                    AuthType::BearerToken
                } else {
                    AuthType::None
                },
                credential_ref,
                headers: Default::default(),
                enabled: true,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };
            create_endpoint(&mut *tx, &endpoint)
                .await
                .map_err(|e| e.to_string())?;
            endpoint_by_native_provider.insert(name.to_string(), endpoint.id);
        }
    }

    if import_models {
        let mut placeholder_endpoint: Option<Uuid> = None;
        for m in &parsed.models {
            let native_provider = m
                .route
                .overrides
                .get("native_provider_id")
                .and_then(|v| v.as_str())
                .unwrap_or("imported");
            let endpoint_id = match endpoint_by_native_provider.get(native_provider) {
                Some(id) => *id,
                None => match placeholder_endpoint {
                    Some(id) => id,
                    None => {
                        let id = imported_endpoint_id(&mut *tx, inst).await?;
                        placeholder_endpoint = Some(id);
                        id
                    }
                },
            };
            let route = ModelRoute::new(
                m.route.remote_model_id.clone(),
                m.route.display_name.clone(),
                m.route.context_window,
                m.route.capabilities.clone(),
                serde_json::json!({
                    "provenance": provenance.clone(),
                    "native": m.route.overrides,
                }),
            );
            let route = ModelRoute {
                endpoint_id,
                ..route
            };
            match create_route(&mut *tx, &route).await {
                Ok(_) => {
                    report.models_imported += 1;
                    let now = Utc::now();
                    upsert_catalog_model(
                        &mut *tx,
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
                Err(e) if is_unique_violation(&e) => report
                    .duplicates
                    .push(format!("model:{}", m.route.remote_model_id)),
                Err(e) => {
                    return Err(format!(
                        "import failed on model {}: {e}",
                        m.route.remote_model_id
                    ));
                }
            }
        }
    }

    if import_mcp {
        for m in &parsed.mcp {
            if existing_mcp.iter().any(|s| s.name == m.server.name)
                || !batch_mcp.insert(m.server.name.clone())
            {
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
            create_mcp_server(&mut *tx, &server)
                .await
                .map_err(|e| e.to_string())?;
            report.mcp_imported += 1;
        }
    }

    if import_skills {
        for s in &parsed.skills {
            if s.symlinked {
                // already canonical; binding is created in Phase 10 — reported separately
                report.skills_symlinked += 1;
                continue;
            }
            if existing_skills.iter().any(|sk| sk.canonical_path == s.path)
                || !batch_skills.insert(s.path.clone())
            {
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
            create_skill(&mut *tx, &skill)
                .await
                .map_err(|e| e.to_string())?;
            report.skills_imported += 1;
        }
    }

    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(report)
}

/// Creates the shared placeholder endpoint used only when an imported route's
/// native provider carried no base_url. Disabled and protocol-agnostic.
async fn imported_endpoint_id(
    tx: &mut sqlx::SqliteConnection,
    inst: &HarnessInstallation,
) -> Result<Uuid, String> {
    // NOTE: reached at most once per import (caller caches the placeholder).
    // Safe against UNIQUE because this path only runs when no "imported"
    // provider existed in the hoisted pre-seed.
    let provider = create_provider(&mut *tx, "imported", "Imported (needs setup)")
        .await
        .map_err(|e| e.to_string())?;
    let endpoints = list_endpoints(&mut *tx, provider.id)
        .await
        .map_err(|e| e.to_string())?;
    if let Some(e) = endpoints.first() {
        return Ok(e.id);
    }
    let endpoint = ProviderEndpoint {
        id: Uuid::new_v4(),
        provider_id: provider.id,
        name: format!("{}-imported", inst.harness_type.as_str()),
        base_url: String::new(),
        protocol: Protocol::Custom,
        discovery_path: None,
        auth_type: AuthType::None,
        credential_ref: None,
        headers: Default::default(),
        enabled: false,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    create_endpoint(&mut *tx, &endpoint)
        .await
        .map_err(|e| e.to_string())?;
    Ok(endpoint.id)
}
