//! Harness-detail model view: disk rows enriched with library linkage, plus
//! adoption of on-device-only models into the library.

use chm_database::repos::models::{create_route, list_catalog_models, list_routes};
use chm_database::repos::providers::{list_endpoints, list_providers};
use serde::Serialize;
use tauri::State;
use uuid::Uuid;

use crate::AppState;
use crate::commands::import::{read_parsed_installation, read_parsed_state};

#[derive(Debug, Clone)]
struct HarnessProviderDeclaration {
    name: String,
    base_url: Option<String>,
    model_ids: std::collections::HashSet<String>,
}

/// The Pi config may carry an API key as a literal, an environment-variable
/// reference, or a shell command. Keep the classification separate from the
/// normalized provider state so secrets never cross the adapter/UI boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
enum HarnessApiKeySource {
    Environment(String),
    Literal(String),
    Command,
}

fn pi_api_key_source(
    raw: &str,
    provider_name: &str,
) -> Result<Option<HarnessApiKeySource>, String> {
    let document: serde_json::Value =
        serde_json::from_str(raw).map_err(|error| format!("invalid Pi models.json: {error}"))?;
    let Some(providers) = document
        .get("providers")
        .and_then(|value| value.as_object())
    else {
        return Ok(None);
    };
    let Some(config) = providers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(provider_name))
        .and_then(|(_, value)| value.as_object())
    else {
        return Ok(None);
    };
    let Some(value) = config
        .get("apiKey")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };

    if let Some(rest) = value.strip_prefix('$') {
        let name = rest
            .strip_prefix('{')
            .and_then(|rest| rest.strip_suffix('}'))
            .unwrap_or(rest)
            .trim();
        let valid = !name.is_empty()
            && name.chars().enumerate().all(|(index, character)| {
                if index == 0 {
                    character == '_' || character.is_ascii_alphabetic()
                } else {
                    character == '_' || character.is_ascii_alphanumeric()
                }
            });
        if valid {
            return Ok(Some(HarnessApiKeySource::Environment(name.to_string())));
        }
    }
    if value.starts_with('!') {
        // Pi supports command-backed credentials, but CHM must not execute
        // arbitrary config commands while importing a provider.
        return Ok(Some(HarnessApiKeySource::Command));
    }
    Ok(Some(HarnessApiKeySource::Literal(value.to_string())))
}

fn pi_models_config_path(
    install: &chm_core::domain::harness::HarnessInstallation,
) -> Option<std::path::PathBuf> {
    let path = install.config_path.as_ref().map(std::path::PathBuf::from)?;
    if path.file_name().and_then(|name| name.to_str()) == Some("models.json") {
        Some(path)
    } else {
        path.parent().map(|parent| parent.join("models.json"))
    }
}

/// Resolve a Pi provider's configured key into CHM's OS-backed credential
/// reference. Literal values are stored in the OS keychain; `$ENV_VAR`
/// values remain environment references. Command-backed values are never
/// executed and therefore return no credential.
async fn harness_api_key_credential(
    state: &AppState,
    install: &chm_core::domain::harness::HarnessInstallation,
    provider_name: &str,
) -> Result<Option<chm_core::domain::credentials::CredentialRef>, String> {
    if install.harness_type.as_str() != "pi" {
        return Ok(None);
    }
    let Some(path) = pi_models_config_path(install) else {
        return Ok(None);
    };
    let raw = std::fs::read_to_string(&path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let Some(source) = pi_api_key_source(&raw, provider_name)? else {
        return Ok(None);
    };
    match source {
        HarnessApiKeySource::Environment(name) => {
            chm_database::repos::providers::create_credential_ref(
                &state.pool,
                chm_core::domain::credentials::CredentialKind::Env,
                &name,
            )
            .await
            .map(Some)
            .map_err(|error| error.to_string())
        }
        HarnessApiKeySource::Literal(value) => {
            let store_key = format!(
                "providers/harness/{}/{}",
                install.id,
                slugify(provider_name)
            );
            state.secrets.set(&store_key, &value).map_err(|error| {
                format!("could not store the Pi API key in the OS keychain: {error}")
            })?;
            let reference = format!("coding-harness-manager/{store_key}");
            chm_database::repos::providers::create_credential_ref(
                &state.pool,
                chm_core::domain::credentials::CredentialKind::Keychain,
                &reference,
            )
            .await
            .map(Some)
            .map_err(|error| error.to_string())
        }
        HarnessApiKeySource::Command => Ok(None),
    }
}

/// Read provider declarations from the adapter's normalized state. Every
/// adapter is allowed to parse a different native format, but provider records
/// share a small safe shape (`native_provider_id` plus optional base URL), and
/// model routes carry the provider id in their overrides. Keeping this logic
/// on normalized state means TOML, JSON, JSONC, and YAML harnesses all receive
/// the same attribution and provider-detail behavior.
fn harness_provider_declarations(
    parsed: &chm_harness_sdk::adapter::types::ParsedState,
) -> Vec<HarnessProviderDeclaration> {
    let mut declarations = Vec::new();

    for provider in &parsed.providers {
        let Some(object) = provider.as_object() else {
            continue;
        };
        // Claude settings also use ParsedState.providers for arbitrary env
        // overrides. Those are not model-serving providers and should never
        // appear in the Models table.
        if object.contains_key("env_override") {
            continue;
        }
        let Some(name) = ["native_provider_id", "name", "id", "provider"]
            .iter()
            .find_map(|key| object.get(*key).and_then(|value| value.as_str()))
            .map(str::trim)
            .filter(|name| !name.is_empty() && !name.starts_with("__"))
        else {
            continue;
        };
        let base_url = ["base_url", "baseUrl", "url", "openAiBaseUrl"]
            .iter()
            .find_map(|key| object.get(*key).and_then(|value| value.as_str()))
            .map(str::to_string);
        let mut model_ids = provider_model_ids(object);

        // Most adapters expose the provider separately from its model list.
        // Reconnect those records through the normalized model override so a
        // provider still attributes correctly when its native config stores
        // models in a sibling table or file.
        for model in &parsed.models {
            if model_provider_id(model)
                .is_some_and(|provider_id| provider_id.eq_ignore_ascii_case(name))
            {
                model_ids.insert(model.native_id.to_lowercase());
                model_ids.insert(model.route.remote_model_id.to_lowercase());
            }
        }

        if let Some(existing) =
            declarations
                .iter_mut()
                .find(|declaration: &&mut HarnessProviderDeclaration| {
                    declaration.name.eq_ignore_ascii_case(name)
                })
        {
            if existing.base_url.is_none() {
                existing.base_url = base_url;
            }
            existing.model_ids.extend(model_ids);
        } else {
            declarations.push(HarnessProviderDeclaration {
                name: name.to_string(),
                base_url,
                model_ids,
            });
        }
    }

    declarations
}

fn provider_model_ids(
    object: &serde_json::Map<String, serde_json::Value>,
) -> std::collections::HashSet<String> {
    let mut model_ids = std::collections::HashSet::new();
    match object.get("models") {
        Some(serde_json::Value::Array(models)) => {
            for model in models {
                match model {
                    serde_json::Value::String(id) => {
                        model_ids.insert(id.to_lowercase());
                    }
                    serde_json::Value::Object(model) => {
                        if let Some(id) = model.get("id").and_then(|value| value.as_str()) {
                            model_ids.insert(id.to_lowercase());
                        }
                    }
                    _ => {}
                }
            }
        }
        Some(serde_json::Value::Object(models)) => {
            model_ids.extend(models.keys().map(|id| id.to_lowercase()));
        }
        _ => {}
    }
    model_ids
}

fn harness_provider_map_from_declarations(
    declarations: &[HarnessProviderDeclaration],
) -> std::collections::HashMap<String, (String, Option<String>)> {
    let mut map = std::collections::HashMap::new();
    for declaration in declarations {
        for model_id in &declaration.model_ids {
            map.entry(model_id.clone())
                .or_insert_with(|| (declaration.name.clone(), declaration.base_url.clone()));
        }
    }
    map
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessModelRow {
    pub native_id: String,
    pub native_provider_id: Option<String>,
    pub remote_model_id: String,
    pub display_name: String,
    pub context_window: Option<i64>,
    pub in_library: bool,
    pub library_route_id: Option<String>,
    pub library_display_name: Option<String>,
    /// Provider serving this model, when we can attribute it.
    pub provider_name: Option<String>,
    /// How the provider was attributed. Best first: "harness" (the harness
    /// config itself groups models under a provider), then "library"
    /// (routed My Model), then "catalog" (exact id in a discovered catalog),
    /// with "-suffix" variants for namespaced ids like `gl/glm-5.2`.
    pub provider_match: Option<String>,
    /// Provider base URL when the attribution came from the harness config.
    pub provider_base_url: Option<String>,
    /// Registry provider id, when the attributed provider exists in the
    /// Providers section (drives the provider-detail link).
    pub provider_id: Option<String>,
}

#[tauri::command]
pub async fn harness_models_view_cmd(
    state: State<'_, AppState>,
    installation_id: String,
) -> Result<Vec<HarnessModelRow>, String> {
    let (_inst, parsed) = read_parsed_installation(&state.pool, &installation_id).await?;
    let routes = list_routes(&state.pool).await.map_err(|e| e.to_string())?;
    let providers = list_providers(&state.pool)
        .await
        .map_err(|e| e.to_string())?;
    let native_provider_declarations = harness_provider_declarations(&parsed);
    let native_providers = harness_provider_map_from_declarations(&native_provider_declarations);

    // endpoint -> (provider id, provider display name); base_url -> provider id.
    let mut endpoint_provider: std::collections::HashMap<uuid::Uuid, (uuid::Uuid, String)> =
        std::collections::HashMap::new();
    let mut base_url_provider: std::collections::HashMap<String, uuid::Uuid> =
        std::collections::HashMap::new();
    let mut provider_endpoints = Vec::new();
    for p in &providers {
        for e in list_endpoints(&state.pool, p.id)
            .await
            .map_err(|e| e.to_string())?
        {
            endpoint_provider.insert(e.id, (p.id, p.display_name.clone()));
            base_url_provider
                .entry(crate::services::normalize_base_url(&e.base_url))
                .or_insert(p.id);
            provider_endpoints.push((p.id, p.display_name.clone(), e));
        }
    }

    // Library attribution: route remote id -> (provider id, name).
    let mut library_provider: std::collections::HashMap<String, (uuid::Uuid, String)> =
        std::collections::HashMap::new();
    for r in &routes {
        if let Some(pidpn) = endpoint_provider.get(&r.endpoint_id) {
            library_provider
                .entry(r.remote_model_id.to_lowercase())
                .or_insert(pidpn.clone());
        }
    }

    // Catalog attribution (remote id across every discovered endpoint).
    let mut catalog_provider: std::collections::HashMap<String, (uuid::Uuid, String)> =
        std::collections::HashMap::new();
    for (provider_id, provider_name, e) in provider_endpoints {
        for c in list_catalog_models(&state.pool, e.id)
            .await
            .map_err(|e| e.to_string())?
        {
            catalog_provider
                .entry(c.remote_model_id.to_lowercase())
                .or_insert((provider_id, provider_name.clone()));
        }
    }

    /// Lookup keys for a harness model id: harnesses commonly prefix gateway
    /// or vendor namespaces onto bare model ids (`gl/glm-5.2`,
    /// `cp/cline-pass/deepseek-v4-flash`), so after the exact id we try each
    /// tail after successive slashes before giving up.
    fn attribution_keys(remote_id: &str) -> Vec<String> {
        let lower = remote_id.to_lowercase();
        let mut keys = vec![lower.clone()];
        let mut rest = lower.as_str();
        while let Some(idx) = rest.find('/') {
            rest = &rest[idx + 1..];
            if rest.is_empty() {
                break;
            }
            keys.push(rest.to_string());
        }
        keys
    }

    Ok(parsed
        .models
        .iter()
        .map(|m| {
            let remote_lower = m.route.remote_model_id.to_lowercase();
            let match_route = routes
                .iter()
                .find(|r| r.remote_model_id.to_lowercase() == remote_lower);
            let keys = attribution_keys(&m.route.remote_model_id);
            let find_in = |map: &std::collections::HashMap<String, (uuid::Uuid, String)>| {
                keys.iter().find_map(|k| map.get(k).cloned())
            };
            let (provider_name, provider_match, provider_base_url, provider_id) = {
                // 1) The harness's own provider grouping is authoritative.
                let native_lower = m.native_id.to_lowercase();
                // Prefer the parsed provider id when a harness exposes the
                // same native id under multiple providers. The fallback map
                // intentionally keeps the first match for legacy rows that
                // do not carry provider metadata, but must not shadow a
                // provider-qualified row.
                let native = m
                    .route
                    .overrides
                    .get("native_provider_id")
                    .and_then(|value| value.as_str())
                    .and_then(|provider| {
                        harness_provider_for_model(
                            &native_provider_declarations,
                            provider,
                            &m.route.remote_model_id,
                            &m.native_id,
                        )
                    })
                    .or_else(|| {
                        keys.first()
                            .and_then(|k| native_providers.get(k))
                            .or_else(|| native_providers.get(&native_lower))
                            .cloned()
                    });
                if let Some((pname, base)) = native {
                    let pid = base
                        .as_deref()
                        .and_then(|b| {
                            base_url_provider.get(&crate::services::normalize_base_url(b))
                        })
                        .copied();
                    let display_name = pid
                        .and_then(|provider_id| {
                            providers
                                .iter()
                                .find(|provider| provider.id == provider_id)
                                .map(|provider| provider.display_name.clone())
                        })
                        .unwrap_or(pname);
                    (
                        Some(display_name),
                        Some("harness".to_string()),
                        base,
                        pid.map(|p| p.to_string()),
                    )
                } else if let Some((pid, pn)) =
                    keys.first().and_then(|k| library_provider.get(k).cloned())
                {
                    (
                        Some(pn),
                        Some("library".to_string()),
                        None,
                        Some(pid.to_string()),
                    )
                } else if let Some((pid, pn)) =
                    keys.first().and_then(|k| catalog_provider.get(k).cloned())
                {
                    (
                        Some(pn),
                        Some("catalog".to_string()),
                        None,
                        Some(pid.to_string()),
                    )
                } else if let Some((pid, pn)) = find_in(&library_provider) {
                    (
                        Some(pn),
                        Some("library-suffix".to_string()),
                        None,
                        Some(pid.to_string()),
                    )
                } else if let Some((pid, pn)) = find_in(&catalog_provider) {
                    (
                        Some(pn),
                        Some("catalog-suffix".to_string()),
                        None,
                        Some(pid.to_string()),
                    )
                } else {
                    (None, None, None, None)
                }
            };
            HarnessModelRow {
                native_id: m.native_id.clone(),
                native_provider_id: m
                    .route
                    .overrides
                    .get("native_provider_id")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                remote_model_id: m.route.remote_model_id.clone(),
                display_name: m.route.display_name.clone(),
                context_window: m.route.context_window,
                in_library: match_route.is_some(),
                library_route_id: match_route.map(|r| r.id.to_string()),
                library_display_name: match_route.map(|r| r.display_name.clone()),
                provider_name,
                provider_match,
                provider_base_url,
                provider_id,
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

/// Shared adopt core: idempotently create a My Model route for the given
/// harness row under the chosen endpoint.
struct AdoptRouteInput<'a> {
    remote_model_id: &'a str,
    display_name: &'a str,
    context_window: Option<i64>,
    max_input: Option<i64>,
    max_output: Option<i64>,
    native_provider_id: Option<&'a str>,
}

async fn adopt_route(
    pool: &sqlx::Pool<sqlx::Sqlite>,
    endpoint: Uuid,
    input: AdoptRouteInput<'_>,
) -> Result<AdoptOutcome, String> {
    let existing = list_routes(pool).await.map_err(|e| e.to_string())?;
    let remote_lower = input.remote_model_id.to_lowercase();
    if let Some(already) = existing
        .iter()
        .find(|r| r.endpoint_id == endpoint && r.remote_model_id.to_lowercase() == remote_lower)
    {
        return Ok(AdoptOutcome {
            route_id: already.id.to_string(),
            created: false,
        });
    }
    let mut overrides = serde_json::json!({ "provenance": { "source": "adopted-from-harness" } });
    if let Some(provider) = input.native_provider_id {
        overrides["native_provider_id"] = serde_json::Value::String(provider.to_string());
    }
    let mut route = chm_core::domain::models::ModelRoute::new(
        input.remote_model_id.to_string(),
        input.display_name.to_string(),
        input.context_window,
        serde_json::json!({}),
        overrides,
    );
    route.endpoint_id = endpoint;
    route.max_input = input.max_input;
    route.max_output = input.max_output;
    let created = create_route(pool, &route)
        .await
        .map_err(|e| e.to_string())?;
    Ok(AdoptOutcome {
        route_id: created.id.to_string(),
        created: true,
    })
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
    native_provider_id: Option<String>,
) -> Result<AdoptOutcome, String> {
    let endpoint = Uuid::parse_str(&endpoint_id).map_err(|e| e.to_string())?;
    let (_id, _htype, parsed) = read_parsed_state(&state.pool, &installation_id).await?;
    let model = parsed
        .models
        .iter()
        .find(|m| {
            m.native_id == native_id
                && native_provider_id.as_deref().is_none_or(|provider| {
                    m.route
                        .overrides
                        .get("native_provider_id")
                        .and_then(|v| v.as_str())
                        .is_some_and(|actual| actual.eq_ignore_ascii_case(provider))
                })
        })
        .ok_or_else(|| format!("model {native_id} not found on this harness"))?;
    adopt_route(
        &state.pool,
        endpoint,
        AdoptRouteInput {
            remote_model_id: &model.route.remote_model_id,
            display_name: &model.route.display_name,
            context_window: model.route.context_window,
            max_input: model.route.max_input,
            max_output: model.route.max_output,
            native_provider_id: model
                .route
                .overrides
                .get("native_provider_id")
                .and_then(|value| value.as_str()),
        },
    )
    .await
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
// --- Targeted harness model edits (edit / delete / duplicate) ---

use chm_core::domain::history::{ConfigSnapshot, TransactionStatus, TransactionType};
use chm_database::repos::history::{add_snapshot, begin_transaction, finish_transaction};
use chm_harness_sdk::adapter::plan::{ActualState, DesiredState, Mode};
use chm_reconciliation::engine::{filter_unsupported, reconcile};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessModelOp {
    /// "update" | "remove" | "duplicate"
    pub op: String,
    pub native_id: String,
    #[serde(default)]
    pub native_provider_id: Option<String>,
    /// Optional destination provider for duplicate. Omitted means preserve
    /// the source provider.
    #[serde(default)]
    pub destination_provider_id: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub context_window: Option<i64>,
    #[serde(default)]
    pub remote_model_id: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessEditReport {
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
    pub unchanged: usize,
    pub files_written: Vec<String>,
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

fn activity_harness_label(value: &str) -> String {
    let mut chars = value.replace('-', " ").chars().collect::<Vec<_>>();
    if let Some(first) = chars.first_mut() {
        first.make_ascii_uppercase();
    }
    chars.into_iter().collect()
}

fn model_provider_id(model: &chm_harness_sdk::adapter::types::HarnessModel) -> Option<&str> {
    model
        .route
        .overrides
        .get("native_provider_id")
        .and_then(|value| value.as_str())
        .or_else(|| {
            model
                .route
                .capabilities
                .get("provider")
                .and_then(|value| value.as_str())
        })
}

fn model_activity_label(model_id: &str, display_name: Option<&str>) -> String {
    let id = activity_value(model_id);
    let display = display_name
        .map(activity_value)
        .filter(|value| !value.is_empty() && !value.eq_ignore_ascii_case(model_id));
    display.map(|name| format!("{id} ({name})")).unwrap_or(id)
}

fn format_context_window(value: Option<i64>) -> String {
    value
        .map(|number| number.to_string())
        .unwrap_or_else(|| "unset".into())
}

fn changed_field_label(field: &str) -> &str {
    match field {
        "context_window" => "context window",
        "max_input" => "max input",
        "max_output" => "max output",
        "display_name" => "display name",
        "capabilities" => "capabilities",
        other => other,
    }
}

fn model_action_matches(
    action: &chm_harness_sdk::adapter::plan::PlanAction,
    expected_action: &str,
    model_id: &str,
    provider: Option<&str>,
) -> bool {
    use chm_harness_sdk::adapter::plan::PlanAction;
    let (kind, identity, action_provider, action_name) = match action {
        PlanAction::Add(value) => (
            value.kind.as_str(),
            value.identity.as_str(),
            value.native_provider_id.as_deref(),
            "add",
        ),
        PlanAction::Update(value) => (
            value.kind.as_str(),
            value.identity.as_str(),
            value.native_provider_id.as_deref(),
            "update",
        ),
        PlanAction::Remove(value) => (
            value.kind.as_str(),
            value.identity.as_str(),
            value.native_provider_id.as_deref(),
            "remove",
        ),
        _ => return false,
    };
    kind == "model"
        && action_name == expected_action
        && identity.eq_ignore_ascii_case(model_id)
        && match (provider, action_provider) {
            (Some(expected), Some(actual)) => expected.eq_ignore_ascii_case(actual),
            (Some(_), None) => false,
            (None, _) => true,
        }
}

fn plan_has_model_action(
    plan: &chm_harness_sdk::adapter::plan::ReconciliationPlan,
    expected_action: &str,
    model_id: &str,
    provider: Option<&str>,
) -> bool {
    plan.actions
        .iter()
        .any(|action| model_action_matches(action, expected_action, model_id, provider))
}

/// Build a readable, secret-free audit description for direct model edits.
/// The old counter summary (`+1 ~0 -0`) is intentionally not used here: the
/// activity feed should identify the model and the provider that changed.
pub(crate) fn model_edit_activity_summary(
    harness_type: &str,
    parsed: &chm_harness_sdk::adapter::types::ParsedState,
    ops: &[HarnessModelOp],
    plan: &chm_harness_sdk::adapter::plan::ReconciliationPlan,
) -> String {
    let harness = activity_harness_label(harness_type);
    let mut details = Vec::new();
    for op in ops {
        let source = parsed.models.iter().find(|model| {
            model.native_id.eq_ignore_ascii_case(&op.native_id)
                && op.native_provider_id.as_deref().is_none_or(|provider| {
                    model_provider_id(model)
                        .is_some_and(|actual| actual.eq_ignore_ascii_case(provider))
                })
        });
        let provider = op
            .destination_provider_id
            .as_deref()
            .or(op.native_provider_id.as_deref())
            .or_else(|| source.and_then(model_provider_id));
        let provider_suffix = provider
            .map(|value| format!(" via {}", activity_value(value)))
            .unwrap_or_default();
        match op.op.as_str() {
            "remove" if plan_has_model_action(plan, "remove", &op.native_id, provider) => {
                let label = source
                    .map(|model| {
                        model_activity_label(&model.native_id, Some(&model.route.display_name))
                    })
                    .unwrap_or_else(|| model_activity_label(&op.native_id, None));
                details.push(format!("Deleted model {label}{provider_suffix}"));
            }
            "duplicate" => {
                let new_id = op
                    .remote_model_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("");
                let new_id = if new_id.is_empty() {
                    format!("{}-copy", op.native_id)
                } else {
                    new_id.to_string()
                };
                if !plan_has_model_action(plan, "add", &new_id, provider) {
                    continue;
                }
                let source_label = source
                    .map(|model| {
                        model_activity_label(&model.native_id, Some(&model.route.display_name))
                    })
                    .unwrap_or_else(|| model_activity_label(&op.native_id, None));
                let new_display = op
                    .display_name
                    .as_deref()
                    .filter(|value| !value.trim().is_empty());
                let default_display =
                    source.map(|model| format!("{} (copy)", model.route.display_name));
                let new_label =
                    model_activity_label(&new_id, new_display.or(default_display.as_deref()));
                details.push(format!(
                    "Duplicated model {source_label} as {new_label}{provider_suffix}"
                ));
            }
            "update" => {
                let renamed_id = op
                    .remote_model_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| {
                        !value.is_empty() && !value.eq_ignore_ascii_case(&op.native_id)
                    });
                let has_plan_change =
                    plan_has_model_action(plan, "update", &op.native_id, provider)
                        || renamed_id.is_some_and(|new_id| {
                            plan_has_model_action(plan, "add", new_id, provider)
                                || plan_has_model_action(plan, "remove", &op.native_id, provider)
                        });
                if !has_plan_change {
                    continue;
                }
                let source_label = source
                    .map(|model| {
                        model_activity_label(&model.native_id, Some(&model.route.display_name))
                    })
                    .unwrap_or_else(|| model_activity_label(&op.native_id, None));
                let mut changes = Vec::new();
                if let Some(new_id) = renamed_id {
                    let renamed_display = op
                        .display_name
                        .as_deref()
                        .or_else(|| source.map(|model| model.route.display_name.as_str()));
                    changes.push(format!(
                        "renamed to {}",
                        model_activity_label(new_id, renamed_display)
                    ));
                } else if let (Some(source), Some(display)) = (source, op.display_name.as_deref())
                    && source.route.display_name != display
                {
                    changes.push(format!(
                        "display name \"{}\" → \"{}\"",
                        activity_value(&source.route.display_name),
                        activity_value(display)
                    ));
                }
                if let Some(context_window) = op.context_window
                    && source.is_none_or(|model| model.route.context_window != Some(context_window))
                {
                    changes.push(format!(
                        "context window {} → {}",
                        source
                            .map(|model| format_context_window(model.route.context_window))
                            .unwrap_or_else(|| "unknown".into()),
                        format_context_window(Some(context_window))
                    ));
                }
                if changes.is_empty()
                    && let Some(fields) = plan.actions.iter().find_map(|action| match action {
                        chm_harness_sdk::adapter::plan::PlanAction::Update(update)
                            if model_action_matches(action, "update", &op.native_id, provider) =>
                        {
                            Some(update.changed_fields.clone())
                        }
                        _ => None,
                    })
                {
                    changes.extend(
                        fields
                            .iter()
                            .map(|field| changed_field_label(field).to_string()),
                    );
                }
                let change_suffix = if changes.is_empty() {
                    String::new()
                } else {
                    format!(": {}", changes.join(", "))
                };
                details.push(format!(
                    "Updated model {source_label}{provider_suffix}{change_suffix}"
                ));
            }
            _ => {}
        }
    }
    if details.is_empty() {
        let add = count_kind(plan, "model", "add");
        let update = count_kind(plan, "model", "update");
        let remove = count_kind(plan, "model", "remove");
        return format!("{harness}: {add} model(s) added, {update} updated, {remove} deleted");
    }
    if details.len() > 8 {
        let remaining = details.len() - 8;
        details.truncate(8);
        details.push(format!("{remaining} more model change(s)"));
    }
    format!("{harness}: {}", details.join("; "))
}

#[tauri::command]
pub async fn apply_harness_model_edits_cmd(
    state: State<'_, AppState>,
    installation_id: String,
    ops: Vec<HarnessModelOp>,
) -> Result<HarnessEditReport, String> {
    if ops.is_empty() {
        return Err("no operations given".into());
    }
    let inst = crate::commands::find_installation(&state.pool, &installation_id).await?;
    let adapter = adapters::all_adapters()
        .into_iter()
        .find(|a| a.id() == inst.harness_type.as_str())
        .ok_or("no adapter for harness")?;
    let parsed = adapter.read_state(&inst).map_err(|e| e.to_string())?;

    // desired = current disk models, modified by the ops. Rows the user did
    // not touch stay byte-identical (Unchanged); a removed row is simply
    // absent from desired, which — combined with its managed flag — makes the
    // reconciler emit a Remove.
    let mut managed: std::collections::HashMap<String, bool> = std::collections::HashMap::new();
    for m in &parsed.models {
        managed.insert(
            format!("route:{}:{}", m.route.endpoint_id, m.native_id),
            true,
        );
    }
    for m in &parsed.mcp {
        managed.insert(format!("mcp:{}", m.native_name), false);
    }
    for sk in &parsed.skills {
        managed.insert(format!("skill:{}", sk.path), false);
    }

    let model_provider = |m: &chm_harness_sdk::adapter::types::HarnessModel| {
        m.route
            .overrides
            .get("native_provider_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase()
    };
    let identity = |provider: &str, id: &str| format!("{provider}\u{1f}{id}");
    let mut used_ids: std::collections::HashSet<String> = parsed
        .models
        .iter()
        .map(|m| identity(&model_provider(m), &m.native_id.to_lowercase()))
        .collect();
    let mut desired_routes: Vec<chm_core::domain::models::ModelRoute> = Vec::new();
    for m in &parsed.models {
        let provider = model_provider(m);
        let candidates: Vec<_> = ops
            .iter()
            .filter(|o| o.native_id == m.native_id)
            .filter(|o| {
                o.native_provider_id
                    .as_deref()
                    .map(|p| p.eq_ignore_ascii_case(&provider))
                    .unwrap_or(true)
            })
            .collect();
        if candidates.len() > 1 {
            return Err(format!(
                "model {} exists under multiple providers; native_provider_id is required",
                m.native_id
            ));
        }
        let op = candidates.first().copied();
        match op.map(|o| o.op.as_str()) {
            Some("remove") => {
                // omitted from desired -> Remove
            }
            Some("duplicate") => {
                let new_id = op
                    .and_then(|o| o.remote_model_id.as_deref())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("{}-copy", m.native_id));
                let destination_provider = op
                    .and_then(|o| o.destination_provider_id.as_deref())
                    .unwrap_or(&provider)
                    .to_string();
                let new_identity =
                    identity(&destination_provider.to_lowercase(), &new_id.to_lowercase());
                if used_ids.contains(&new_identity) {
                    return Err(format!(
                        "a model named \"{new_id}\" already exists for provider \"{destination_provider}\" on this harness"
                    ));
                }
                let display = op
                    .and_then(|o| o.display_name.as_deref())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("{} (copy)", m.route.display_name));
                let mut copy = m.route.clone();
                copy.remote_model_id = new_id.clone();
                copy.display_name = display;
                copy.overrides["native_provider_id"] =
                    serde_json::Value::String(destination_provider.clone());
                used_ids.insert(new_identity);
                desired_routes.push(copy);
                let mut kept = m.route.clone();
                kept.remote_model_id = m.native_id.clone();
                desired_routes.push(kept);
            }
            Some("update") | None => {
                let mut route = m.route.clone();
                route.remote_model_id = m.native_id.clone();
                if let Some(o) = op {
                    if let Some(dn) = &o.display_name {
                        route.display_name = dn.clone();
                    }
                    if o.context_window.is_some() {
                        route.context_window = o.context_window;
                    }
                    if let Some(rm) = &o.remote_model_id {
                        let rm = rm.trim().to_string();
                        if !rm.is_empty() && rm != m.native_id {
                            let renamed_identity = identity(&provider, &rm.to_lowercase());
                            let source_identity = identity(&provider, &m.native_id.to_lowercase());
                            if renamed_identity != source_identity
                                && used_ids.contains(&renamed_identity)
                            {
                                return Err(format!(
                                    "a model named \"{rm}\" already exists for provider \"{provider}\" on this harness"
                                ));
                            }
                            // rename: add under the new id; the old native id
                            // drops out of desired -> reconciler removes it.
                            used_ids.remove(&source_identity);
                            used_ids.insert(renamed_identity);
                            route.remote_model_id = rm;
                        }
                    }
                }
                desired_routes.push(route);
            }
            Some(other) => return Err(format!("unknown op {other}")),
        }
    }

    let desired = DesiredState {
        routes: desired_routes,
        mcp_servers: vec![],
        skills: vec![],
    };
    let actual = ActualState {
        routes: parsed.models.clone(),
        mcp: parsed.mcp.clone(),
        skills: parsed.skills.clone(),
        managed_flags: managed,
    };
    let plan = reconcile(&desired, &actual, Mode::ReplaceManaged).map_err(|e| e.to_string())?;
    let caps = adapter.capabilities();
    let plan = filter_unsupported(plan, &caps);
    let native_plan = adapter.plan(&plan, &inst).map_err(|e| e.to_string())?;

    let mutating = count_kind(&plan, "model", "add")
        + count_kind(&plan, "model", "update")
        + count_kind(&plan, "model", "remove");
    if mutating == 0 {
        return Err("nothing changed by these operations".into());
    }
    // The adapter may drop actions it cannot write (e.g. an older writer
    // without removal support). Reporting success while writing nothing is
    // worse than failing loudly with the adapter's own explanation.
    if native_plan.changes.is_empty() {
        let detail = if native_plan.warnings.is_empty() {
            "the adapter produced no writable changes for this operation".to_string()
        } else {
            native_plan.warnings.join("; ")
        };
        return Err(format!(
            "this harness adapter cannot write this change yet: {detail}"
        ));
    }

    let tx = begin_transaction(
        &state.pool,
        TransactionType::Manual,
        serde_json::json!({ "reason": "harness model edit", "plan": native_plan }),
    )
    .await
    .map_err(|e| e.to_string())?;

    // backups first — all-or-nothing before any mutation
    let mut backups: Vec<(String, std::path::PathBuf)> = Vec::new();
    for change in &native_plan.changes {
        match chm_filesystem::backup_file(std::path::Path::new(&change.file_path)) {
            Ok(b) => backups.push((change.file_path.clone(), b)),
            Err(e) => {
                let msg = format!("backup failed before write: {e}");
                let _ = finish_transaction(
                    &state.pool,
                    tx.id,
                    TransactionStatus::Failed,
                    None,
                    Some(msg.clone()),
                )
                .await;
                return Err(msg);
            }
        }
    }

    let apply_outcome = adapter
        .apply(&inst, &native_plan)
        .map_err(|e| e.to_string());
    match apply_outcome {
        Ok(apply_result) => {
            for (file, backup) in &backups {
                let before = std::fs::read_to_string(backup).ok();
                let after = std::fs::read_to_string(file).ok();
                let hash = crate::drift::sha256_hex;
                if let Err(error) = add_snapshot(
                    &state.pool,
                    &ConfigSnapshot {
                        id: Uuid::new_v4(),
                        transaction_id: tx.id,
                        harness_installation_id: inst.id,
                        path: file.clone(),
                        before_content: before.clone(),
                        after_content: after.clone(),
                        before_hash: before.as_deref().map(hash),
                        after_hash: after.as_deref().map(hash),
                    },
                )
                .await
                {
                    let msg = format!("could not record edit snapshot: {error}");
                    let rollback_error = rollback_manual_edit(
                        &state.pool,
                        tx.id,
                        &*adapter,
                        &inst,
                        &native_plan,
                        &backups,
                        &msg,
                    )
                    .await
                    .err();
                    return Err(rollback_error.unwrap_or(msg));
                }
            }
            let validation = match adapter.validate(&inst) {
                Ok(validation) => validation,
                Err(error) => {
                    let msg = format!("validation failed: {error}");
                    let rollback_error = rollback_manual_edit(
                        &state.pool,
                        tx.id,
                        &*adapter,
                        &inst,
                        &native_plan,
                        &backups,
                        &msg,
                    )
                    .await
                    .err();
                    return Err(rollback_error.unwrap_or(msg));
                }
            };
            if validation.ok {
                if let Err(error) = finish_transaction(
                    &state.pool,
                    tx.id,
                    TransactionStatus::Succeeded,
                    Some(model_edit_activity_summary(
                        inst.harness_type.as_str(),
                        &parsed,
                        &ops,
                        &plan,
                    )),
                    None,
                )
                .await
                {
                    let _ = finish_transaction(
                        &state.pool,
                        tx.id,
                        TransactionStatus::Failed,
                        None,
                        Some(format!("could not finish edit transaction: {error}")),
                    )
                    .await;
                    return Err(error.to_string());
                }
                Ok(HarnessEditReport {
                    files_written: apply_result.files_written,
                    added: count_kind(&plan, "model", "add"),
                    updated: count_kind(&plan, "model", "update"),
                    removed: count_kind(&plan, "model", "remove"),
                    unchanged: count_kind(&plan, "model", "unchanged"),
                })
            } else {
                let msg = format!("validation failed: {:?}", validation.errors);
                let rollback_error = rollback_manual_edit(
                    &state.pool,
                    tx.id,
                    &*adapter,
                    &inst,
                    &native_plan,
                    &backups,
                    &msg,
                )
                .await
                .err();
                if let Some(error) = rollback_error {
                    return Err(format!("{msg}; {error}"));
                }
                Err(msg)
            }
        }
        Err(e) => {
            let rollback_error = rollback_manual_edit(
                &state.pool,
                tx.id,
                &*adapter,
                &inst,
                &native_plan,
                &backups,
                &e,
            )
            .await
            .err();
            if let Some(error) = rollback_error {
                return Err(format!("{e}; {error}"));
            }
            Err(e)
        }
    }
}

/// Restore a failed direct-edit transaction and always close its audit row.
/// Returning recovery failures is important: a failed restore can leave the
/// harness in a mixed state and must not be presented as a clean edit error.
async fn rollback_manual_edit(
    pool: &sqlx::Pool<sqlx::Sqlite>,
    tx_id: Uuid,
    adapter: &dyn chm_harness_sdk::adapter::types::HarnessAdapter,
    install: &chm_core::domain::harness::HarnessInstallation,
    native_plan: &chm_harness_sdk::adapter::types::NativePlan,
    backups: &[(String, std::path::PathBuf)],
    reason: &str,
) -> Result<(), String> {
    let errors = [reason.to_owned()];
    crate::services::transactions::rollback_native_transaction(
        pool,
        tx_id,
        adapter,
        install,
        native_plan,
        backups,
        &errors,
    )
    .await
}

fn count_kind(
    plan: &chm_harness_sdk::adapter::plan::ReconciliationPlan,
    kind: &str,
    action: &str,
) -> usize {
    use chm_harness_sdk::adapter::plan::PlanAction;
    plan.actions
        .iter()
        .filter(|a| match a {
            PlanAction::Add(x) => x.kind == kind && action == "add",
            PlanAction::Update(x) => x.kind == kind && action == "update",
            PlanAction::Remove(x) => x.kind == kind && action == "remove",
            PlanAction::Unchanged(x) => x.kind == kind && action == "unchanged",
            _ => false,
        })
        .count()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartAdoptOutcome {
    pub route_id: String,
    pub route_created: bool,
    pub provider_created: bool,
    pub endpoint_created: bool,
    pub provider_name: String,
    pub endpoint_id: String,
}

async fn find_endpoint_by_base_url(
    pool: &sqlx::Pool<sqlx::Sqlite>,
    providers: &[chm_core::domain::provider::Provider],
    base_url: &str,
) -> Result<Option<chm_core::domain::provider::ProviderEndpoint>, String> {
    let target_base = crate::services::normalize_base_url(base_url);
    for provider in providers {
        let endpoints = list_endpoints(pool, provider.id)
            .await
            .map_err(|e| e.to_string())?;
        if let Some(endpoint) = endpoints
            .into_iter()
            .find(|endpoint| crate::services::normalize_base_url(&endpoint.base_url) == target_base)
        {
            return Ok(Some(endpoint));
        }
    }
    Ok(None)
}

fn slugify(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let trimmed = s.trim_matches('-').to_lowercase();
    if trimmed.is_empty() {
        "provider".into()
    } else {
        trimmed
    }
}

/// One-click import for models whose harness config declares the serving
/// provider (name + base URL). Reuses an existing endpoint with the same
/// base URL, or creates the provider + endpoint on the fly, then routes the
/// model. Falls back to an error when the harness config has no provider
/// info — the UI then shows the manual endpoint picker instead.
#[tauri::command]
pub async fn smart_adopt_harness_model_cmd(
    state: State<'_, AppState>,
    installation_id: String,
    native_id: String,
    native_provider_id: Option<String>,
) -> Result<SmartAdoptOutcome, String> {
    use chm_core::domain::provider::{AuthType, Protocol, ProviderEndpoint};

    let (inst, parsed) = read_parsed_installation(&state.pool, &installation_id).await?;
    let model = parsed
        .models
        .iter()
        .find(|m| {
            m.native_id == native_id
                && native_provider_id.as_deref().is_none_or(|provider| {
                    m.route
                        .overrides
                        .get("native_provider_id")
                        .and_then(|v| v.as_str())
                        .is_some_and(|actual| actual.eq_ignore_ascii_case(provider))
                })
        })
        .ok_or_else(|| format!("model {native_id} not found on this harness"))?;

    let provider_declarations = harness_provider_declarations(&parsed);
    let provider_map = harness_provider_map_from_declarations(&provider_declarations);
    let native_lower = native_id.to_lowercase();
    let provider = native_provider_id
        .as_deref()
        .and_then(|provider| {
            harness_provider_for_model(
                &provider_declarations,
                provider,
                &model.route.remote_model_id,
                &native_id,
            )
        })
        .or_else(|| {
            native_providers_lookup(
                &provider_map,
                &model.route.remote_model_id.to_lowercase(),
                &native_lower,
            )
            .cloned()
        });
    let Some((provider_name, Some(base_url))) = provider else {
        return Err(
            "this harness config does not declare a provider for this model; choose an endpoint manually"
                .into(),
        );
    };

    // Existing endpoint with the same base URL?
    let providers = list_providers(&state.pool)
        .await
        .map_err(|e| e.to_string())?;
    let mut provider_created = false;
    let mut endpoint_created = false;
    let endpoint = find_endpoint_by_base_url(&state.pool, &providers, &base_url).await?;
    let credential = if endpoint
        .as_ref()
        .is_none_or(|endpoint| endpoint.credential_ref.is_none())
    {
        harness_api_key_credential(&state, &inst, &provider_name).await?
    } else {
        None
    };

    let endpoint = if let Some(mut e) = endpoint {
        if e.credential_ref.is_none()
            && let Some(credential) = credential
        {
            e.credential_ref = Some(credential);
            e.auth_type = AuthType::BearerToken;
            e.updated_at = chrono::Utc::now();
            e = chm_database::repos::providers::update_endpoint(&state.pool, &e)
                .await
                .map_err(|error| error.to_string())?;
        }
        e
    } else {
        // Create the provider (reuse by slug when present) and its endpoint.
        provider_created = true;
        endpoint_created = true;
        let slug = slugify(&provider_name);
        let provider = match providers.into_iter().find(|p| p.name == slug) {
            Some(p) => p,
            None => {
                chm_database::repos::providers::create_provider(&state.pool, &slug, &provider_name)
                    .await
                    .map_err(|e| e.to_string())?
            }
        };
        chm_database::repos::providers::create_endpoint(
            &state.pool,
            &ProviderEndpoint {
                id: Uuid::new_v4(),
                provider_id: provider.id,
                name: "API".into(),
                base_url: base_url.clone(),
                protocol: Protocol::parse_str("openai-chat"),
                discovery_path: Some("/v1/models".into()),
                auth_type: AuthType::BearerToken,
                credential_ref: credential,
                headers: Default::default(),
                enabled: true,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            },
        )
        .await
        .map_err(|e| e.to_string())?
    };

    let outcome = adopt_route(
        &state.pool,
        endpoint.id,
        AdoptRouteInput {
            remote_model_id: &model.route.remote_model_id,
            display_name: &model.route.display_name,
            context_window: model.route.context_window,
            max_input: model.route.max_input,
            max_output: model.route.max_output,
            native_provider_id: model
                .route
                .overrides
                .get("native_provider_id")
                .and_then(|value| value.as_str())
                .or(Some(provider_name.as_str())),
        },
    )
    .await?;
    Ok(SmartAdoptOutcome {
        route_id: outcome.route_id,
        route_created: outcome.created,
        provider_created,
        endpoint_created,
        provider_name: provider_name.clone(),
        endpoint_id: endpoint.id.to_string(),
    })
}

fn native_providers_lookup<'m>(
    map: &'m std::collections::HashMap<String, (String, Option<String>)>,
    remote_lower: &str,
    native_lower: &str,
) -> Option<&'m (String, Option<String>)> {
    map.get(remote_lower).or_else(|| map.get(native_lower))
}

/// Resolve a provider-specific model directly from the harness config. The
/// fallback map intentionally keeps the first provider for display, but
/// adoption must not do that when two providers expose the same native id.
fn harness_provider_for_model(
    declarations: &[HarnessProviderDeclaration],
    provider_id: &str,
    remote_model_id: &str,
    native_id: &str,
) -> Option<(String, Option<String>)> {
    let remote_lower = remote_model_id.to_lowercase();
    let native_lower = native_id.to_lowercase();
    declarations
        .iter()
        .find(|declaration| {
            declaration.name.eq_ignore_ascii_case(provider_id)
                && (declaration.model_ids.contains(&remote_lower)
                    || declaration.model_ids.contains(&native_lower))
        })
        .map(|declaration| (declaration.name.clone(), declaration.base_url.clone()))
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnsureProviderOutcome {
    pub provider_id: String,
    pub provider_created: bool,
    pub endpoint_created: bool,
    pub credential_attached: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessProviderDetail {
    pub installation_id: String,
    pub harness_type: String,
    pub provider_name: String,
    pub base_url: Option<String>,
    pub models: Vec<String>,
    pub attribution_confidence: String,
}

/// Read-only provider detail for a provider that exists in a harness config
/// but has not yet been added to CHM's canonical provider registry. This
/// command deliberately performs no database writes.
#[tauri::command]
pub async fn harness_provider_detail_cmd(
    state: State<'_, AppState>,
    installation_id: String,
    provider_name: String,
) -> Result<HarnessProviderDetail, String> {
    let (inst, parsed) = read_parsed_installation(&state.pool, &installation_id).await?;
    // Parse the provider declaration once. Calling harness_provider_for_model
    // inside the model iterator reread and reparsed the same config for every
    // row, which was needlessly expensive for larger harness configs.
    let (declared_name, base_url, declared_model_ids) =
        harness_provider_models(&parsed, &provider_name).ok_or_else(|| {
            format!("provider {provider_name} not declared in this harness config")
        })?;
    let models = parsed
        .models
        .iter()
        .filter(|m| {
            let declared_id = declared_model_ids.contains(&m.route.remote_model_id.to_lowercase())
                || declared_model_ids.contains(&m.native_id.to_lowercase());
            let native_provider = m
                .route
                .overrides
                .get("native_provider_id")
                .and_then(|value| value.as_str())
                .map(|provider| provider.eq_ignore_ascii_case(&declared_name));
            declared_id && native_provider.is_none_or(|matches| matches)
        })
        .map(|m| m.route.remote_model_id.clone())
        .collect();
    Ok(HarnessProviderDetail {
        installation_id,
        harness_type: inst.harness_type.as_str().to_string(),
        provider_name: declared_name,
        base_url,
        models,
        attribution_confidence: "declared by harness config".into(),
    })
}

/// Return one harness provider declaration and its model ids, parsing the
/// config a single time. Provider model lists may be arrays or id-keyed
/// objects depending on the harness.
fn harness_provider_models(
    parsed: &chm_harness_sdk::adapter::types::ParsedState,
    provider_name: &str,
) -> Option<(String, Option<String>, std::collections::HashSet<String>)> {
    harness_provider_declarations(parsed)
        .into_iter()
        .find(|declaration| declaration.name.eq_ignore_ascii_case(provider_name))
        .map(|declaration| {
            (
                declaration.name,
                declaration.base_url,
                declaration.model_ids,
            )
        })
}

/// Materialize a harness-declared provider (name + base URL from the
/// harness's own config) into the registry so it has a detail page.
/// Reuses the provider by slug and the endpoint by base URL when present.
#[tauri::command]
pub async fn ensure_provider_from_harness_cmd(
    state: State<'_, AppState>,
    installation_id: String,
    provider_name: String,
) -> Result<EnsureProviderOutcome, String> {
    use chm_core::domain::provider::{AuthType, Protocol, ProviderEndpoint};

    let (inst, parsed) = read_parsed_installation(&state.pool, &installation_id).await?;

    let provider_declarations = harness_provider_declarations(&parsed);
    let base_url = provider_declarations
        .iter()
        .find(|declaration| declaration.name.eq_ignore_ascii_case(&provider_name))
        .and_then(|declaration| declaration.base_url.clone())
        .ok_or_else(|| format!("provider {provider_name} not declared in this harness's config"))?;

    let providers = list_providers(&state.pool)
        .await
        .map_err(|e| e.to_string())?;
    let mut provider_created = false;
    let mut endpoint_created = false;

    let endpoint = find_endpoint_by_base_url(&state.pool, &providers, &base_url).await?;
    let credential = if endpoint
        .as_ref()
        .is_none_or(|endpoint| endpoint.credential_ref.is_none())
    {
        harness_api_key_credential(&state, &inst, &provider_name).await?
    } else {
        None
    };
    let mut credential_attached = false;

    let provider = if let Some(e) = &endpoint {
        providers
            .iter()
            .find(|p| p.id == e.provider_id)
            .cloned()
            .ok_or("endpoint without provider")?
    } else {
        provider_created = true;
        endpoint_created = true;
        let slug = slugify(&provider_name);
        match providers.into_iter().find(|p| p.name == slug) {
            Some(p) => {
                provider_created = false;
                p
            }
            None => {
                chm_database::repos::providers::create_provider(&state.pool, &slug, &provider_name)
                    .await
                    .map_err(|e| e.to_string())?
            }
        }
    };

    let _endpoint_id = if let Some(mut e) = endpoint {
        if e.credential_ref.is_none()
            && let Some(credential) = credential
        {
            e.credential_ref = Some(credential);
            e.auth_type = AuthType::BearerToken;
            e.updated_at = chrono::Utc::now();
            chm_database::repos::providers::update_endpoint(&state.pool, &e)
                .await
                .map_err(|error| error.to_string())?;
            credential_attached = true;
        }
        e.id
    } else {
        credential_attached = credential.is_some();
        chm_database::repos::providers::create_endpoint(
            &state.pool,
            &ProviderEndpoint {
                id: Uuid::new_v4(),
                provider_id: provider.id,
                name: "API".into(),
                base_url,
                protocol: Protocol::parse_str("openai-chat"),
                discovery_path: Some("/v1/models".into()),
                auth_type: AuthType::BearerToken,
                credential_ref: credential,
                headers: Default::default(),
                enabled: true,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            },
        )
        .await
        .map_err(|e| e.to_string())?
        .id
    };

    Ok(EnsureProviderOutcome {
        provider_id: provider.id.to_string(),
        provider_created,
        endpoint_created,
        credential_attached,
    })
}

#[cfg(test)]
mod activity_tests {
    use super::{
        HarnessApiKeySource, HarnessModelOp, harness_provider_declarations,
        harness_provider_for_model, harness_provider_models, model_edit_activity_summary,
        pi_api_key_source,
    };
    use chm_core::domain::models::ModelRoute;
    use chm_harness_sdk::adapter::plan::{PlanAction, ReconciliationPlan, RemoveAction};
    use chm_harness_sdk::adapter::types::{HarnessModel, ParsedState};

    #[test]
    fn model_edit_summary_names_deleted_model_and_provider() {
        let route = ModelRoute::new(
            "qwen3.8-27b".into(),
            "Qwen 3.8 27B".into(),
            Some(32768),
            serde_json::json!({}),
            serde_json::json!({"native_provider_id": "Yolo-Auto"}),
        );
        let parsed = ParsedState {
            models: vec![HarnessModel {
                native_id: "qwen3.8-27b".into(),
                route,
            }],
            ..Default::default()
        };
        let ops = vec![HarnessModelOp {
            op: "remove".into(),
            native_id: "qwen3.8-27b".into(),
            native_provider_id: None,
            destination_provider_id: None,
            display_name: None,
            context_window: None,
            remote_model_id: None,
        }];
        let plan = ReconciliationPlan {
            actions: vec![PlanAction::Remove(RemoveAction {
                kind: "model".into(),
                identity: "qwen3.8-27b".into(),
                native_provider_id: Some("Yolo-Auto".into()),
            })],
        };
        let summary = model_edit_activity_summary("pi", &parsed, &ops, &plan);
        assert_eq!(
            summary,
            "Pi: Deleted model qwen3.8-27b (Qwen 3.8 27B) via Yolo-Auto"
        );
    }

    #[test]
    fn provider_attribution_uses_normalized_adapter_provider_records() {
        let route = ModelRoute::new(
            "deepseek-v4-flash".into(),
            "custom-api-cline-bot/deepseek-v4-flash".into(),
            Some(1_000_000),
            serde_json::json!({"provider": "custom-api-cline-bot"}),
            serde_json::json!({}),
        );
        let parsed = ParsedState {
            models: vec![HarnessModel {
                native_id: "deepseek-v4-flash".into(),
                route,
            }],
            providers: vec![
                serde_json::json!({
                    "native_provider_id": "custom-api-cline-bot",
                    "base_url": "https://api.cline.bot/api/v1"
                }),
                serde_json::json!({"native_provider_id": "__schema__"}),
            ],
            ..Default::default()
        };

        let declarations = harness_provider_declarations(&parsed);
        assert_eq!(declarations.len(), 1, "schema metadata is not a provider");
        assert_eq!(declarations[0].name, "custom-api-cline-bot");
        assert_eq!(
            declarations[0].base_url.as_deref(),
            Some("https://api.cline.bot/api/v1")
        );
        assert!(declarations[0].model_ids.contains("deepseek-v4-flash"));

        let resolved = harness_provider_for_model(
            &declarations,
            "custom-api-cline-bot",
            "deepseek-v4-flash",
            "deepseek-v4-flash",
        )
        .expect("provider should resolve from normalized records");
        assert_eq!(resolved.0, "custom-api-cline-bot");
        assert_eq!(resolved.1.as_deref(), Some("https://api.cline.bot/api/v1"));

        let detail = harness_provider_models(&parsed, "custom-api-cline-bot")
            .expect("provider detail should use the same declaration source");
        assert_eq!(detail.0, "custom-api-cline-bot");
        assert_eq!(detail.2.len(), 1);
    }

    #[test]
    fn pi_api_key_source_classifies_without_exposing_secrets() {
        let literal = r#"{
            "providers": {"Yolo-Auto": {"apiKey": "secret-value"}}
        }"#;
        assert!(matches!(
            pi_api_key_source(literal, "yolo-auto"),
            Ok(Some(HarnessApiKeySource::Literal(_)))
        ));

        let env = r#"{
            "providers": {"Yolo-Auto": {"apiKey": "$YOLO_API_KEY"}}
        }"#;
        assert_eq!(
            pi_api_key_source(env, "Yolo-Auto"),
            Ok(Some(HarnessApiKeySource::Environment(
                "YOLO_API_KEY".into()
            )))
        );

        let command = r#"{
            "providers": {"Yolo-Auto": {"apiKey": "!security find-generic-password"}}
        }"#;
        assert_eq!(
            pi_api_key_source(command, "Yolo-Auto"),
            Ok(Some(HarnessApiKeySource::Command))
        );
    }
}
