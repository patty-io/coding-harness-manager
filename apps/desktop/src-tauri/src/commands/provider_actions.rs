//! Provider health-check + model discovery commands (Phase 5.4).
//!
//! The provider-level discovery flow is opinionated about endpoint selection:
//!
//! * A provider that exposes both Anthropic- and OpenAI-compatible endpoints
//!   typically returns the **same model ids** on both. Probing both doubles
//!   the catalog without adding information, so we probe **at most one
//!   endpoint per protocol family**, preferring OpenAI-chat (it's the
//!   canonical catalog format that downstream tooling understands).
//! * Per-endpoint discovery is still available via `discover_endpoint_models`
//!   for users who want to inspect every endpoint independently.

use chm_core::domain::models::{ModelRoute, ProviderCatalogModel};
use chm_core::domain::provider::{Protocol, ProviderEndpoint};
use chm_database::repos::models::{
    create_route, list_catalog_models, list_routes, upsert_catalog_model,
};
use chm_database::repos::providers::list_endpoints;
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
    Ok(status.as_str().to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverReport {
    pub total: usize,
    pub added: usize,
    pub updated: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredModel {
    pub catalog_id: String,
    pub endpoint_id: String,
    pub endpoint_name: String,
    pub provider_name: String,
    pub remote_model_id: String,
    pub display_name: Option<String>,
    pub context_length: Option<i64>,
    pub status: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointDiscoverOutcome {
    pub endpoint_id: String,
    pub endpoint_name: String,
    pub report: Option<DiscoverReport>,
    pub error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDiscoverReport {
    pub endpoints_attempted: usize,
    pub endpoints_succeeded: usize,
    pub endpoints_failed: usize,
    pub endpoints_skipped: Vec<SkippedEndpoint>,
    pub total: usize,
    pub added: usize,
    pub updated: usize,
    pub distinct_models: usize,
    pub new_models: Vec<DiscoveredModel>,
    pub updated_models: Vec<DiscoveredModel>,
    pub outcomes: Vec<EndpointDiscoverOutcome>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedEndpoint {
    pub endpoint_id: String,
    pub endpoint_name: String,
    pub reason: String,
}

fn count_failed_discovery_outcomes(
    outcomes: &[EndpointDiscoverOutcome],
    skipped: &[SkippedEndpoint],
) -> usize {
    outcomes
        .iter()
        .filter(|outcome| {
            outcome.error.is_some()
                && outcome.report.is_none()
                && !skipped
                    .iter()
                    .any(|endpoint| endpoint.endpoint_id == outcome.endpoint_id)
        })
        .count()
}

/// Pick the endpoints that should be probed during a provider-level discovery.
/// One endpoint per protocol family — preferring OpenAI-chat first because it
/// is the canonical catalog format. Endpoints that fail to load a credential
/// or have no discovery path are skipped with an explicit reason.
fn pick_discovery_endpoints(
    endpoints: &[ProviderEndpoint],
) -> (Vec<ProviderEndpoint>, Vec<(ProviderEndpoint, String)>) {
    const PRIORITY: &[Protocol] = &[
        Protocol::OpenAiChatCompletions,
        Protocol::OpenAiResponses,
        Protocol::OpenRouterOpenAi,
        Protocol::AnthropicMessages,
        Protocol::Custom,
    ];
    let mut chosen: Vec<ProviderEndpoint> = Vec::new();
    let mut skipped: Vec<(ProviderEndpoint, String)> = Vec::new();
    let mut chosen_protocols: Vec<Protocol> = Vec::new();
    for &proto in PRIORITY {
        for ep in endpoints.iter().filter(|e| e.enabled) {
            if !chosen_protocols.contains(&proto) && ep.protocol == proto {
                chosen.push(ep.clone());
                chosen_protocols.push(proto);
            }
        }
    }
    // Anything that wasn't picked goes into the skipped list with an explanation.
    for ep in endpoints.iter().filter(|e| e.enabled) {
        if !chosen.iter().any(|c| c.id == ep.id) {
            let same_protocol_chosen = chosen.iter().any(|c| c.protocol == ep.protocol);
            let reason = if same_protocol_chosen {
                format!(
                    "another {} endpoint on this provider was probed instead; both protocols return the same catalog on most providers",
                    ep.protocol.as_str()
                )
            } else {
                format!(
                    "protocol {} not in discovery priority order",
                    ep.protocol.as_str()
                )
            };
            skipped.push((ep.clone(), reason));
        }
    }
    (chosen, skipped)
}

async fn discover_into_catalog(
    pool: &sqlx::Pool<sqlx::Sqlite>,
    endpoint_id: Uuid,
    endpoint: &ProviderEndpoint,
    cred: Option<&str>,
    http: &reqwest::Client,
) -> Result<DiscoverReport, String> {
    let models = discover_models(endpoint, cred, http)
        .await
        .map_err(|e| e.to_string())?;
    let existing = list_catalog_models(pool, endpoint_id)
        .await
        .map_err(|e| e.to_string())?;
    let mut added = 0;
    let mut updated = 0;
    let now = chrono::Utc::now();
    for m in &models {
        let is_new = !existing.iter().any(|c| c.remote_model_id == m.id);
        upsert_catalog_model(
            pool,
            &ProviderCatalogModel {
                id: Uuid::new_v4(),
                endpoint_id,
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
pub async fn discover_endpoint_models(
    state: State<'_, AppState>,
    endpoint_id: String,
) -> Result<DiscoverReport, String> {
    let id = Uuid::parse_str(&endpoint_id).map_err(|e| e.to_string())?;
    let endpoint = find_endpoint(&state.pool, id).await?;
    let cred = resolve_endpoint_credential(&endpoint, state.secrets.as_ref());
    discover_into_catalog(&state.pool, id, &endpoint, cred.as_deref(), &state.http).await
}

fn extract_display_name(raw: &serde_json::Value, fallback_id: &str) -> Option<String> {
    raw.get("display_name")
        .or_else(|| raw.get("displayName"))
        .or_else(|| raw.get("name"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s != fallback_id)
}

fn extract_context_length(raw: &serde_json::Value) -> Option<i64> {
    raw.get("context_length")
        .or_else(|| raw.get("contextLength"))
        .or_else(|| raw.get("max_context_window_tokens"))
        .and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)))
        .filter(|n| *n > 0)
}

#[tauri::command]
pub async fn discover_provider_models(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<ProviderDiscoverReport, String> {
    let id = Uuid::parse_str(&provider_id).map_err(|e| e.to_string())?;
    let provider = chm_database::repos::providers::list_providers(&state.pool)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| format!("provider {provider_id} not found"))?;
    let endpoints = list_endpoints(&state.pool, id)
        .await
        .map_err(|e| e.to_string())?;
    let (chosen, skipped) = pick_discovery_endpoints(&endpoints);
    let endpoints_attempted = chosen.len();
    let mut outcomes: Vec<EndpointDiscoverOutcome> = skipped
        .iter()
        .map(|(ep, reason)| EndpointDiscoverOutcome {
            endpoint_id: ep.id.to_string(),
            endpoint_name: ep.name.clone(),
            report: None,
            error: Some(reason.clone()),
        })
        .collect();
    let mut total = 0usize;
    let mut added = 0usize;
    let mut updated = 0usize;
    let mut succeeded = 0usize;
    let mut new_models: Vec<DiscoveredModel> = Vec::new();
    let mut updated_models: Vec<DiscoveredModel> = Vec::new();
    for ep in chosen {
        let cred = resolve_endpoint_credential(&ep, state.secrets.as_ref());
        match discover_into_catalog(&state.pool, ep.id, &ep, cred.as_deref(), &state.http).await {
            Ok(report) => {
                total += report.total;
                added += report.added;
                updated += report.updated;
                succeeded += 1;
                // Fetch the freshly-upserted rows so we can report id + metadata
                // to the UI without a separate roundtrip.
                let rows = list_catalog_models(&state.pool, ep.id)
                    .await
                    .unwrap_or_default();
                let was_added = report.added;
                let was_updated = report.updated;
                let _ = (was_added, was_updated);
                for row in rows {
                    let display_name =
                        extract_display_name(&row.raw_metadata, &row.remote_model_id);
                    let context_length = extract_context_length(&row.raw_metadata);
                    let is_new = row.status == chm_core::domain::models::CatalogStatus::New;
                    let model = DiscoveredModel {
                        catalog_id: row.id.to_string(),
                        endpoint_id: ep.id.to_string(),
                        endpoint_name: ep.name.clone(),
                        provider_name: provider.name.clone(),
                        remote_model_id: row.remote_model_id.clone(),
                        display_name,
                        context_length,
                        status: row.status.as_str().to_string(),
                    };
                    if is_new {
                        new_models.push(model);
                    } else {
                        updated_models.push(model);
                    }
                }
                outcomes.push(EndpointDiscoverOutcome {
                    endpoint_id: ep.id.to_string(),
                    endpoint_name: ep.name.clone(),
                    report: Some(report),
                    error: None,
                });
            }
            Err(err) => {
                outcomes.push(EndpointDiscoverOutcome {
                    endpoint_id: ep.id.to_string(),
                    endpoint_name: ep.name.clone(),
                    report: None,
                    error: Some(err),
                });
            }
        }
    }
    let skipped_reports: Vec<SkippedEndpoint> = skipped
        .into_iter()
        .map(|(ep, reason)| SkippedEndpoint {
            endpoint_id: ep.id.to_string(),
            endpoint_name: ep.name,
            reason,
        })
        .collect();
    let failed = count_failed_discovery_outcomes(&outcomes, &skipped_reports);
    let distinct_models = new_models.len() + updated_models.len();
    Ok(ProviderDiscoverReport {
        endpoints_attempted,
        endpoints_succeeded: succeeded,
        endpoints_failed: failed,
        endpoints_skipped: skipped_reports,
        total,
        added,
        updated,
        distinct_models,
        new_models,
        updated_models,
        outcomes,
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

/// One deduplicated model entry on a provider. Models belong to the provider;
/// endpoints are just access paths, so catalog rows from duplicate-protocol
/// endpoints are collapsed and the representative row comes from the highest
/// priority endpoint family (openai-chat first).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCatalogEntry {
    pub catalog_id: String,
    pub endpoint_id: String,
    pub endpoint_name: String,
    pub remote_model_id: String,
    pub display_name: Option<String>,
    pub context_length: Option<i64>,
    pub status: String,
    pub last_seen_at: String,
    pub in_my_models: bool,
    pub route_id: Option<String>,
}

#[tauri::command]
pub async fn list_provider_catalog_cmd(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<Vec<ProviderCatalogEntry>, String> {
    let _ = Uuid::parse_str(&provider_id).map_err(|e| e.to_string())?;
    let pool = &state.pool;
    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            Option<String>,
        ),
    >(
        "SELECT c.id, c.endpoint_id, e.name, e.protocol, c.remote_model_id,
                c.raw_metadata_json, c.status, c.last_seen_at, r.id
         FROM provider_catalog_models c
         JOIN provider_endpoints e ON e.id = c.endpoint_id
         LEFT JOIN model_routes r
                ON LOWER(r.endpoint_id) = LOWER(c.endpoint_id)
               AND LOWER(r.remote_model_id) = LOWER(c.remote_model_id)
         WHERE LOWER(e.provider_id) = LOWER(?1)",
    )
    .bind(provider_id.clone())
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    const PRIORITY: &[&str] = &[
        "openai-chat",
        "openai-responses",
        "openrouter-openai",
        "anthropic-messages",
        "custom",
    ];
    let proto_rank = |p: &str| PRIORITY.iter().position(|x| *x == p).unwrap_or(99);

    // Dedupe by remote_model_id, keeping the row from the best-ranked endpoint;
    // a model is "in My Models" if ANY endpoint of this provider routes it.
    use std::collections::HashMap;
    let mut best: HashMap<String, (i32, ProviderCatalogEntry)> = HashMap::new();
    let mut routed: HashMap<String, String> = HashMap::new();
    for (
        catalog_id,
        endpoint_id,
        endpoint_name,
        protocol,
        remote_model_id,
        raw_json,
        status,
        last_seen,
        route_id,
    ) in rows
    {
        let model_key = remote_model_id.to_lowercase();
        if let Some(rid) = &route_id {
            routed
                .entry(model_key.clone())
                .or_insert_with(|| rid.clone());
        }
        let raw: serde_json::Value =
            serde_json::from_str(&raw_json).unwrap_or(serde_json::Value::Null);
        let entry = ProviderCatalogEntry {
            catalog_id,
            endpoint_id,
            endpoint_name,
            remote_model_id: remote_model_id.clone(),
            display_name: extract_display_name(&raw, &remote_model_id),
            context_length: extract_context_length(&raw),
            status,
            last_seen_at: last_seen,
            in_my_models: false,
            route_id: None,
        };
        let rank = proto_rank(&protocol) as i32;
        match best.get(&model_key) {
            Some((existing_rank, _)) if *existing_rank <= rank => {}
            _ => {
                best.insert(model_key, (rank, entry));
            }
        }
    }
    let mut out: Vec<ProviderCatalogEntry> = best
        .into_iter()
        .map(|(mid, (_, mut e))| {
            e.in_my_models = routed.contains_key(&mid);
            e.route_id = routed.get(&mid).cloned();
            e
        })
        .collect();
    out.sort_by(|a, b| a.remote_model_id.cmp(&b.remote_model_id));
    Ok(out)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddToMyModelsReport {
    pub requested: usize,
    pub created: usize,
    pub already_routed: usize,
    pub failures: Vec<String>,
}

#[tauri::command]
pub async fn add_discovered_to_my_models_cmd(
    state: State<'_, AppState>,
    catalog_ids: Vec<String>,
) -> Result<AddToMyModelsReport, String> {
    let pool = &state.pool;
    let mut report = AddToMyModelsReport {
        requested: catalog_ids.len(),
        created: 0,
        already_routed: 0,
        failures: Vec::new(),
    };
    let routes = list_routes(pool).await.map_err(|e| e.to_string())?;
    let mut already_keys: std::collections::HashSet<(Uuid, String)> = routes
        .iter()
        .map(|r| (r.endpoint_id, r.remote_model_id.clone()))
        .collect();
    for catalog_id_str in catalog_ids {
        let catalog_id = match Uuid::parse_str(&catalog_id_str) {
            Ok(id) => id,
            Err(err) => {
                report
                    .failures
                    .push(format!("bad catalog id {catalog_id_str}: {err}"));
                continue;
            }
        };
        let Some(row) = lookup_catalog_by_id(pool, catalog_id).await? else {
            report
                .failures
                .push(format!("catalog row {catalog_id_str} not found"));
            continue;
        };
        if already_keys.contains(&(row.endpoint_id, row.remote_model_id.clone())) {
            report.already_routed += 1;
            continue;
        }
        let display_name = extract_display_name(&row.raw_metadata, &row.remote_model_id)
            .unwrap_or_else(|| row.remote_model_id.clone());
        let context_window = extract_context_length(&row.raw_metadata);
        let route = ModelRoute::new(
            row.remote_model_id.clone(),
            display_name,
            context_window,
            row.raw_metadata.clone(),
            serde_json::json!({}),
        );
        let mut route = route;
        route.endpoint_id = row.endpoint_id;
        match create_route(pool, &route).await {
            Ok(_) => {
                report.created += 1;
                already_keys.insert((route.endpoint_id, route.remote_model_id));
            }
            Err(err) => report
                .failures
                .push(format!("{}: {err}", row.remote_model_id)),
        }
    }
    Ok(report)
}

async fn lookup_catalog_by_id(
    pool: &sqlx::Pool<sqlx::Sqlite>,
    id: Uuid,
) -> Result<Option<chm_core::domain::models::ProviderCatalogModel>, String> {
    use chm_core::domain::models::{CatalogStatus, ProviderCatalogModel};
    let row = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            Option<String>,
            Option<i64>,
            String,
            String,
            Option<String>,
            String,
        ),
    >(
        "SELECT id, endpoint_id, remote_model_id, raw_metadata_json,
                canonical_model_id, match_confidence, first_seen_at, last_seen_at,
                missing_since, status
         FROM provider_catalog_models WHERE id = ?",
    )
    .bind(id.to_string())
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(row.and_then(
        |(id, ep, rid, raw, canonical, confidence, first, last, missing, status)| {
            Some(ProviderCatalogModel {
                id: Uuid::parse_str(&id).ok()?,
                endpoint_id: Uuid::parse_str(&ep).ok()?,
                remote_model_id: rid,
                raw_metadata: serde_json::from_str(&raw).unwrap_or(serde_json::Value::Null),
                canonical_model_id: canonical.and_then(|s| Uuid::parse_str(&s).ok()),
                match_confidence: confidence.map(|c| c as u8),
                first_seen_at: chrono::DateTime::parse_from_rfc3339(&first)
                    .ok()?
                    .with_timezone(&chrono::Utc),
                last_seen_at: chrono::DateTime::parse_from_rfc3339(&last)
                    .ok()?
                    .with_timezone(&chrono::Utc),
                missing_since: missing
                    .and_then(|m| chrono::DateTime::parse_from_rfc3339(&m).ok())
                    .map(|m| m.with_timezone(&chrono::Utc)),
                status: CatalogStatus::parse_str(&status),
            })
        },
    ))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSummary {
    pub endpoints: usize,
    pub discovered_models: usize,
    pub my_models: usize,
    pub health: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSummaryEntry {
    pub provider_id: String,
    pub summary: ProviderSummary,
}

#[tauri::command]
pub async fn provider_summary(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<ProviderSummary, String> {
    let _ = Uuid::parse_str(&provider_id).map_err(|e| e.to_string())?;
    let pool = &state.pool;
    let row = sqlx::query_as::<_, (i64, i64, i64)>(
        "SELECT
           (SELECT COUNT(*) FROM provider_endpoints WHERE LOWER(provider_id) = LOWER(?1)),
           (SELECT COUNT(DISTINCT c.remote_model_id) FROM provider_catalog_models c
              JOIN provider_endpoints e ON e.id = c.endpoint_id
              WHERE LOWER(e.provider_id) = LOWER(?1)),
           (SELECT COUNT(*) FROM model_routes r JOIN provider_endpoints e ON e.id = r.endpoint_id WHERE LOWER(e.provider_id) = LOWER(?1))",
    )
    .bind(provider_id.clone())
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(ProviderSummary {
        endpoints: row.0 as usize,
        discovered_models: row.1 as usize,
        my_models: row.2 as usize,
        health: "unknown".into(), // persisted health lands with Phase 13 doctor
    })
}

/// Batch form used by the provider list. Keeping the aggregation in one
/// query avoids one IPC/database round-trip per provider card.
#[tauri::command]
pub async fn provider_summaries(
    state: State<'_, AppState>,
) -> Result<Vec<ProviderSummaryEntry>, String> {
    let rows = sqlx::query_as::<_, (String, i64, i64, i64)>(
        "SELECT p.id,
                COUNT(DISTINCT e.id),
                COUNT(DISTINCT c.remote_model_id),
                COUNT(DISTINCT r.id)
         FROM providers p
         LEFT JOIN provider_endpoints e ON e.provider_id = p.id
         LEFT JOIN provider_catalog_models c ON c.endpoint_id = e.id
         LEFT JOIN model_routes r ON r.endpoint_id = e.id
         GROUP BY p.id",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(
            |(provider_id, endpoints, discovered_models, my_models)| ProviderSummaryEntry {
                provider_id,
                summary: ProviderSummary {
                    endpoints: endpoints as usize,
                    discovered_models: discovered_models as usize,
                    my_models: my_models as usize,
                    health: "unknown".into(),
                },
            },
        )
        .collect())
}

#[cfg(test)]
mod tests {
    use super::{
        DiscoverReport, EndpointDiscoverOutcome, SkippedEndpoint, count_failed_discovery_outcomes,
    };

    #[test]
    fn failed_discovery_count_excludes_skipped_endpoints() {
        let outcomes = vec![
            EndpointDiscoverOutcome {
                endpoint_id: "failed".into(),
                endpoint_name: "Yolo-Auto API".into(),
                report: None,
                error: Some("authentication failed".into()),
            },
            EndpointDiscoverOutcome {
                endpoint_id: "skipped".into(),
                endpoint_name: "alternate".into(),
                report: None,
                error: Some("duplicate protocol".into()),
            },
            EndpointDiscoverOutcome {
                endpoint_id: "ok".into(),
                endpoint_name: "working".into(),
                report: Some(DiscoverReport {
                    total: 1,
                    added: 1,
                    updated: 0,
                }),
                error: None,
            },
        ];
        let skipped = vec![SkippedEndpoint {
            endpoint_id: "skipped".into(),
            endpoint_name: "alternate".into(),
            reason: "duplicate protocol".into(),
        }];
        assert_eq!(count_failed_discovery_outcomes(&outcomes, &skipped), 1);
    }
}
