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
    pub created_provider_names: Vec<String>,
    pub imported_model_ids: Vec<String>,
    pub imported_mcp_names: Vec<String>,
    pub imported_skill_names: Vec<String>,
    pub duplicates: Vec<String>,
}

fn activity_value(value: &str) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = compact.chars();
    let short = chars.by_ref().take(120).collect::<String>();
    if chars.next().is_some() {
        format!("{short}…")
    } else {
        short
    }
}

fn activity_names(values: &[String]) -> String {
    let mut names = values
        .iter()
        .take(8)
        .map(|value| activity_value(value))
        .collect::<Vec<_>>();
    if values.len() > names.len() {
        names.push(format!("{} more", values.len() - names.len()));
    }
    names.join(", ")
}

fn activity_harness_label(value: &str) -> String {
    let mut chars = value.replace('-', " ").chars().collect::<Vec<_>>();
    if let Some(first) = chars.first_mut() {
        first.make_ascii_uppercase();
    }
    chars.into_iter().collect()
}

impl ImportReport {
    /// A safe history summary for a harness import. It names imported
    /// resources without serializing native config or credential values.
    pub fn activity_summary(&self, harness_type: &str) -> String {
        let mut parts = Vec::new();
        if !self.created_provider_names.is_empty() {
            parts.push(format!(
                "providers: {}",
                activity_names(&self.created_provider_names)
            ));
        }
        if !self.imported_model_ids.is_empty() {
            parts.push(format!(
                "models: {}",
                activity_names(&self.imported_model_ids)
            ));
        }
        if !self.imported_mcp_names.is_empty() {
            parts.push(format!(
                "MCP servers: {}",
                activity_names(&self.imported_mcp_names)
            ));
        }
        if !self.imported_skill_names.is_empty() {
            parts.push(format!(
                "skills: {}",
                activity_names(&self.imported_skill_names)
            ));
        }
        let harness = activity_harness_label(harness_type);
        if parts.is_empty() {
            return format!(
                "{harness}: no new registry items imported ({} duplicate(s))",
                self.duplicates.len()
            );
        }
        if !self.duplicates.is_empty() {
            parts.push(format!("skipped {} duplicate(s)", self.duplicates.len()));
        }
        format!("{harness}: imported {}", parts.join("; "))
    }
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
    let mut endpoints_by_provider: std::collections::HashMap<Uuid, Vec<ProviderEndpoint>> =
        std::collections::HashMap::new();
    // Preload every endpoint. Re-imports must match a harness-declared base
    // URL rather than whichever endpoint happens to sort first; providers can
    // legitimately expose several gateways.
    for p in &existing_providers {
        let endpoints = list_endpoints(pool, p.id)
            .await
            .map_err(|e| e.to_string())?;
        endpoints_by_provider.insert(p.id, endpoints);
    }
    let mut processed_providers: std::collections::HashSet<String> = Default::default();
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
        if !processed_providers.insert(name.to_string()) {
            report.duplicates.push(format!("provider:{name}"));
            continue;
        }
        let provider = if let Some(existing) = existing_providers.iter().find(|p| p.name == name) {
            report.duplicates.push(format!("provider:{name}"));
            existing.clone()
        } else {
            let created = create_provider(&mut *tx, name, name)
                .await
                .map_err(|e| e.to_string())?;
            report.providers_created += 1;
            report.created_provider_names.push(name.to_string());
            created
        };

        let base_url = pv
            .get("base_url")
            .and_then(|v| v.as_str())
            .map(String::from);
        let env_key = pv
            .get("env_key")
            .or_else(|| pv.get("env_reference"))
            .and_then(|v| v.as_str());
        let endpoints = endpoints_by_provider.entry(provider.id).or_default();
        let matching_endpoint = base_url.as_deref().and_then(|base| {
            endpoints
                .iter()
                .find(|endpoint| {
                    crate::services::normalize_base_url(&endpoint.base_url)
                        == crate::services::normalize_base_url(base)
                })
                .cloned()
        });
        let matching_endpoint = matching_endpoint.or_else(|| {
            if base_url.is_none() {
                endpoints.first().cloned()
            } else {
                None
            }
        });
        let endpoint = if let Some(mut endpoint) = matching_endpoint {
            // A provider may already exist from a previous one-click
            // materialization, before the parser learned its explicit Pi
            // `$ENV_VAR` reference. Fill only a missing credential; never
            // replace a credential the user configured in CHM.
            if endpoint.credential_ref.is_none()
                && let Some(key) = env_key.filter(|value| !value.trim().is_empty())
            {
                let credential = create_credential_ref(&mut *tx, CredentialKind::Env, key)
                    .await
                    .map_err(|e| e.to_string())?;
                sqlx::query(
                    "UPDATE provider_endpoints
                     SET auth_type = ?, credential_ref_id = ?, updated_at = ?
                     WHERE id = ?",
                )
                .bind(AuthType::BearerToken.as_str())
                .bind(credential.id.to_string())
                .bind(Utc::now().to_rfc3339())
                .bind(endpoint.id.to_string())
                .execute(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
                endpoint.auth_type = AuthType::BearerToken;
                endpoint.credential_ref = Some(credential);
                endpoint.updated_at = Utc::now();
            }
            endpoint
        } else if let Some(base_url) = base_url {
            let credential_ref: Option<CredentialRef> = match env_key {
                Some(key) => Some(
                    create_credential_ref(&mut *tx, CredentialKind::Env, key)
                        .await
                        .map_err(|e| e.to_string())?,
                ),
                None => None,
            };
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
            endpoints.push(endpoint.clone());
            endpoint
        } else {
            // A provider with no declared base URL cannot be safely mapped to
            // a new gateway. Models will use the shared disabled placeholder
            // endpoint below, preserving the import without inventing a URL.
            continue;
        };
        endpoint_by_native_provider.insert(name.to_string(), endpoint.id);
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
                        let id = imported_endpoint_id(&mut tx, inst).await?;
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
                    report.imported_model_ids.push(m.route.remote_model_id.clone());
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
            report.imported_mcp_names.push(m.server.name.clone());
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
            report.imported_skill_names.push(s.name.clone());
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
    // Reuse an existing placeholder provider when a previous import created
    // it without an endpoint; otherwise a second import would hit the unique
    // provider-name constraint instead of remaining idempotent.
    let provider_id =
        sqlx::query_scalar::<_, String>("SELECT id FROM providers WHERE name = 'imported' LIMIT 1")
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
    let provider_id = match provider_id {
        Some(id) => Uuid::parse_str(&id).map_err(|e| e.to_string())?,
        None => {
            create_provider(&mut *tx, "imported", "Imported (needs setup)")
                .await
                .map_err(|e| e.to_string())?
                .id
        }
    };
    let endpoints = list_endpoints(&mut *tx, provider_id)
        .await
        .map_err(|e| e.to_string())?;
    if let Some(e) = endpoints.first() {
        return Ok(e.id);
    }
    let endpoint = ProviderEndpoint {
        id: Uuid::new_v4(),
        provider_id,
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

#[cfg(test)]
mod tests {
    use super::ImportReport;

    #[test]
    fn activity_summary_names_imported_resources_and_duplicates() {
        let report = ImportReport {
            providers_created: 1,
            models_imported: 1,
            mcp_imported: 1,
            skills_imported: 0,
            skills_symlinked: 0,
            created_provider_names: vec!["Yolo-Auto".into()],
            imported_model_ids: vec!["qwen3.8-27b".into()],
            imported_mcp_names: vec!["github".into()],
            imported_skill_names: vec![],
            duplicates: vec!["model:existing".into()],
        };
        assert_eq!(
            report.activity_summary("pi"),
            "Pi: imported providers: Yolo-Auto; models: qwen3.8-27b; MCP servers: github; skipped 1 duplicate(s)"
        );
    }
}
