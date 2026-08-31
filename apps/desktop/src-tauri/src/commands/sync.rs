//! Sync flow: desired -> actual -> plan -> native plan -> preview/apply -> verify.

use adapters::all_adapters;
use chm_core::domain::credentials::CredentialRef;
use chm_core::domain::harness::{
    BindingType, HarnessInstallation, HarnessMcpBinding, HarnessModelBinding, HarnessSkillBinding,
};
use chm_core::domain::history::{ConfigSnapshot, TransactionStatus, TransactionType};
use chm_core::domain::models::ModelRoute;
use chm_core::domain::provider::{AuthType, Provider, ProviderEndpoint};
use chm_database::repos::harness::{list_model_bindings, upsert_model_binding};
use chm_database::repos::history::{add_snapshot, begin_transaction, finish_transaction};
use chm_database::repos::mcp::{list_mcp_bindings, list_mcp_servers, upsert_mcp_binding};
use chm_database::repos::models::list_routes;
use chm_database::repos::providers::{list_endpoints, list_providers};
use chm_database::repos::skills::{list_skill_bindings, list_skills, upsert_skill_binding};
use chm_filesystem::backup_file;
use chm_harness_sdk::adapter::plan::{
    ActualState, DesiredState, Mode, PlanAction, ReconciliationPlan,
};
use chm_harness_sdk::adapter::route::{CredentialRequirement, ProviderRouteBundle};
use chm_harness_sdk::adapter::types::{ApplyResult, HarnessAdapter, NativePlan, ValidationReport};
use chm_reconciliation::engine::filter_unsupported;
use chm_secrets::SecretStore;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Sqlite};
use tauri::State;
use uuid::Uuid;

use crate::AppState;
use crate::services::credential_deployment::{self, AppliedFile};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionView {
    pub kind: String,
    pub identity: String,
    pub action: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePreview {
    pub path: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewReport {
    pub summary: String,
    pub actions: Vec<ActionView>,
    pub files: Vec<FilePreview>,
    pub plan_hash: String,
    pub writable_changes: usize,
    pub protected_changes: usize,
    pub has_blockers: bool,
    pub warnings: Vec<String>,
    pub route_blockers: Vec<RouteBlockerView>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteBlockerView {
    pub provider_id: String,
    pub model_ids: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyncSelection {
    #[serde(default)]
    pub model_ids: Vec<String>,
    #[serde(default)]
    pub mcp_ids: Vec<String>,
    #[serde(default)]
    pub skill_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyReport {
    pub summary: String,
    pub files_written: Vec<String>,
    pub links_created: Vec<String>,
    pub transaction_id: String,
    pub validation: ValidationReport,
}

pub fn adapter_for(harness_type: &str) -> Option<Box<dyn HarnessAdapter>> {
    all_adapters().into_iter().find(|a| a.id() == harness_type)
}

pub fn parse_mode(s: &str) -> Mode {
    match s {
        "replaceManaged" => Mode::ReplaceManaged,
        _ => Mode::Append,
    }
}

fn effective_mode(mode: &str, selection: Option<&SyncSelection>) -> Result<Mode, String> {
    if selection.is_some() && matches!(parse_mode(mode), Mode::ReplaceManaged) {
        return Err(
            "selection-scoped sync only supports Append; choose the full library scope for Replace Managed"
                .into(),
        );
    }
    Ok(if selection.is_some() {
        Mode::Append
    } else {
        parse_mode(mode)
    })
}

pub(crate) fn group_provider_routes(
    routes: &[ModelRoute],
    providers: &[Provider],
    endpoints: &[ProviderEndpoint],
) -> Result<Vec<ProviderRouteBundle>, String> {
    let provider_by_id = providers
        .iter()
        .map(|provider| (provider.id, provider))
        .collect::<std::collections::HashMap<_, _>>();
    let endpoint_by_id = endpoints
        .iter()
        .map(|endpoint| (endpoint.id, endpoint))
        .collect::<std::collections::HashMap<_, _>>();
    let mut bundle_index = std::collections::HashMap::<Uuid, usize>::new();
    let mut bundles = Vec::<ProviderRouteBundle>::new();

    for route in routes {
        let endpoint = endpoint_by_id.get(&route.endpoint_id).ok_or_else(|| {
            format!(
                "model {} references endpoint {} which does not exist",
                route.remote_model_id, route.endpoint_id
            )
        })?;
        let provider = provider_by_id.get(&endpoint.provider_id).ok_or_else(|| {
            format!(
                "endpoint {} for model {} references provider {} which does not exist",
                endpoint.id, route.remote_model_id, endpoint.provider_id
            )
        })?;
        let credential = if endpoint.auth_type == AuthType::None {
            CredentialRequirement::None
        } else {
            CredentialRequirement::Secret {
                credential_ref: endpoint.credential_ref.clone().ok_or_else(|| {
                    format!(
                        "endpoint {} for model {} requires {} authentication but has no credential",
                        endpoint.name,
                        route.remote_model_id,
                        endpoint.auth_type.as_str()
                    )
                })?,
                auth_type: endpoint.auth_type,
            }
        };

        if let Some(index) = bundle_index.get(&endpoint.id).copied() {
            bundles[index].models.push(route.clone());
        } else {
            bundle_index.insert(endpoint.id, bundles.len());
            bundles.push(ProviderRouteBundle {
                provider_id: provider.name.clone(),
                display_name: provider.display_name.clone(),
                endpoint_id: endpoint.id,
                base_url: endpoint.base_url.clone(),
                protocol: endpoint.protocol,
                credential,
                models: vec![route.clone()],
            });
        }
    }

    Ok(bundles)
}

pub(crate) fn route_for_provider_bundle(
    mut route: ModelRoute,
    bundle: &ProviderRouteBundle,
) -> ModelRoute {
    if !route.overrides.is_object() {
        route.overrides = serde_json::json!({});
    }
    let overrides = route.overrides.as_object_mut().expect("overrides object");
    overrides.insert(
        "native_provider_id".into(),
        serde_json::Value::String(bundle.provider_id.clone()),
    );
    overrides.insert(
        "base_url".into(),
        serde_json::Value::String(bundle.base_url.clone()),
    );
    overrides.insert(
        "protocol".into(),
        serde_json::Value::String(bundle.protocol.as_str().into()),
    );
    let mut config = serde_json::json!({
        "display_name": bundle.display_name,
        "base_url": bundle.base_url,
        "protocol": bundle.protocol.as_str(),
        "endpoint_id": bundle.endpoint_id,
    });
    match &bundle.credential {
        CredentialRequirement::None => {}
        CredentialRequirement::Secret { credential_ref, .. } => {
            config["credential_ref_id"] = serde_json::Value::String(credential_ref.id.to_string());
            config["credential_kind"] = serde_json::Value::String(credential_ref.kind.as_str().into());
            config["credential_reference"] =
                serde_json::Value::String(credential_ref.reference.clone());
            if credential_ref.kind == chm_core::domain::credentials::CredentialKind::Env {
                overrides.insert(
                    "env_key".into(),
                    serde_json::Value::String(credential_ref.reference.clone()),
                );
                overrides.insert(
                    "api_key_env".into(),
                    serde_json::Value::String(credential_ref.reference.clone()),
                );
                config["api_key_env"] = serde_json::Value::String(credential_ref.reference.clone());
            }
        }
    }
    overrides.insert("native_provider_config".into(), config);
    route
}

async fn desired_state(
    pool: &Pool<Sqlite>,
    selection: Option<&SyncSelection>,
) -> Result<DesiredState, String> {
    let route_ids = selection.map(|s| s.model_ids.iter().collect::<std::collections::HashSet<_>>());
    let mcp_ids = selection.map(|s| s.mcp_ids.iter().collect::<std::collections::HashSet<_>>());
    let skill_ids = selection.map(|s| s.skill_ids.iter().collect::<std::collections::HashSet<_>>());
    let providers = list_providers(pool).await.map_err(|e| e.to_string())?;
    let mut endpoints = Vec::new();
    for provider in &providers {
        endpoints.extend(
            list_endpoints(pool, provider.id)
                .await
                .map_err(|e| e.to_string())?,
        );
    }
    let routes = list_routes(pool)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|r| r.enabled)
        .filter(|r| {
            route_ids
                .as_ref()
                .is_none_or(|ids| ids.contains(&r.id.to_string()))
        })
        .collect::<Vec<_>>();
    let provider_routes = group_provider_routes(&routes, &providers, &endpoints)?;
    let routes = provider_routes
        .iter()
        .flat_map(|bundle| {
            bundle
                .models
                .iter()
                .cloned()
                .map(|route| route_for_provider_bundle(route, bundle))
        })
        .collect();
    Ok(DesiredState {
        provider_routes,
        routes,
        mcp_servers: list_mcp_servers(pool)
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|m| m.enabled)
            .filter(|m| {
                mcp_ids
                    .as_ref()
                    .is_none_or(|ids| ids.contains(&m.id.to_string()))
            })
            .collect(),
        skills: list_skills(pool)
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|s| s.enabled)
            .filter(|s| {
                skill_ids
                    .as_ref()
                    .is_none_or(|ids| ids.contains(&s.id.to_string()))
            })
            .collect(),
    })
}

/// managed_flags from the binding tables for this installation. A binding is
/// the durable ownership record used by Replace Managed; native rows without
/// one are deliberately preserved.
async fn managed_flags_for(
    pool: &Pool<Sqlite>,
    install: &HarnessInstallation,
    parsed: &chm_harness_sdk::adapter::types::ParsedState,
) -> Result<std::collections::HashMap<String, bool>, String> {
    let mut flags = std::collections::HashMap::new();
    let bindings = list_model_bindings(pool, install.id)
        .await
        .map_err(|e| e.to_string())?;
    for m in &parsed.models {
        let provider = native_provider_id(&m.route).unwrap_or("");
        let managed = bindings.iter().any(|b| {
            let binding_provider = b
                .native_config
                .get("native_provider_id")
                .and_then(|value| value.as_str())
                .or_else(|| {
                    b.native_config
                        .get("native")
                        .and_then(|value| value.get("native_provider_id"))
                        .and_then(|value| value.as_str())
                });
            b.native_id.eq_ignore_ascii_case(&m.native_id)
                && b.managed
                // A missing provider is only a wildcard for a harness row
                // that also has no provider. Otherwise an old providerless
                // binding could mark duplicate ids under every provider as
                // managed and Replace Managed could remove user-owned rows.
                && match (binding_provider, provider) {
                    (Some(binding), current) => binding.eq_ignore_ascii_case(current),
                    (None, "") => true,
                    (None, _) => false,
                }
        });
        flags.insert(
            format!("route:{}:{}", m.route.endpoint_id, m.native_id),
            managed,
        );
        flags.insert(
            format!("model:{provider}:{}", m.native_id.to_lowercase()),
            managed,
        );
    }
    let mcp_bindings = list_mcp_bindings(pool, install.id)
        .await
        .map_err(|e| e.to_string())?;
    for m in &parsed.mcp {
        flags.insert(
            format!("mcp:{}", m.native_name),
            mcp_bindings
                .iter()
                .any(|b| b.native_name == m.native_name && b.managed),
        );
    }
    let skill_bindings = list_skill_bindings(pool, install.id)
        .await
        .map_err(|e| e.to_string())?;
    for s in &parsed.skills {
        flags.insert(
            format!("skill:{}", s.path),
            skill_bindings
                .iter()
                .any(|b| b.target_path == s.path && b.managed),
        );
    }
    Ok(flags)
}

fn native_provider_id(route: &chm_core::domain::models::ModelRoute) -> Option<&str> {
    route
        .overrides
        .get("native_provider_id")
        .and_then(|v| v.as_str())
        .or_else(|| {
            route
                .overrides
                .get("native")
                .and_then(|v| v.get("native_provider_id"))
                .and_then(|v| v.as_str())
        })
}

fn route_base_url(route: &ModelRoute) -> Option<&str> {
    route
        .overrides
        .get("base_url")
        .and_then(|value| value.as_str())
        .or_else(|| {
            route
                .overrides
                .get("native_provider_config")
                .and_then(|config| config.get("base_url"))
                .and_then(|value| value.as_str())
        })
        .or_else(|| {
            route
                .overrides
                .get("native")
                .and_then(|native| native.get("base_url"))
                .and_then(|value| value.as_str())
        })
}

fn normalized_base_url(value: &str) -> String {
    value.trim().trim_end_matches('/').to_ascii_lowercase()
}

/// Persist the ownership records for resources that are present after a
/// successful write. We only bind rows that the adapter can read back, so a
/// no-op/unsupported action never becomes falsely managed.
async fn record_bindings(
    pool: &Pool<Sqlite>,
    install: &HarnessInstallation,
    desired: &DesiredState,
    plan: &ReconciliationPlan,
    parsed: &chm_harness_sdk::adapter::types::ParsedState,
) -> Result<(), String> {
    let now = chrono::Utc::now();
    for route in &desired.routes {
        let changed_by_sync = plan.actions.iter().any(|action| match action {
            PlanAction::Add(add) => {
                add.kind == "model" && add.identity.eq_ignore_ascii_case(&route.remote_model_id)
            }
            PlanAction::Update(update) => {
                update.kind == "model"
                    && update.identity.eq_ignore_ascii_case(&route.remote_model_id)
            }
            _ => false,
        });
        if !changed_by_sync {
            continue;
        }
        let provider = native_provider_id(route);
        if let Some(actual) = parsed.models.iter().find(|m| {
            if !m
                .route
                .remote_model_id
                .eq_ignore_ascii_case(&route.remote_model_id)
            {
                return false;
            }
            let same_provider = native_provider_id(&m.route)
                .map(|p| provider.is_some_and(|wanted| p.eq_ignore_ascii_case(wanted)))
                .unwrap_or(provider.is_none());
            same_provider
                || matches!(
                    (route_base_url(&m.route), route_base_url(route)),
                    (Some(actual), Some(desired))
                        if normalized_base_url(actual) == normalized_base_url(desired)
                )
        }) {
            let native_config = serde_json::json!({
                "native_provider_id": provider,
                "remote_model_id": route.remote_model_id,
                "base_url": route_base_url(route),
                "protocol": route.overrides.get("protocol"),
                "endpoint_id": route
                    .overrides
                    .get("native_provider_config")
                    .and_then(|config| config.get("endpoint_id")),
                "credential_ref_id": route
                    .overrides
                    .get("native_provider_config")
                    .and_then(|config| config.get("credential_ref_id")),
            });
            upsert_model_binding(
                pool,
                &HarnessModelBinding {
                    id: Uuid::new_v4(),
                    harness_installation_id: install.id,
                    model_route_id: route.id,
                    native_id: actual.native_id.clone(),
                    native_config,
                    managed: true,
                    created_at: now,
                    updated_at: now,
                },
            )
            .await
            .map_err(|e| e.to_string())?;
        }
    }
    for server in &desired.mcp_servers {
        let changed_by_sync = plan.actions.iter().any(|action| match action {
            PlanAction::Add(add) => add.kind == "mcp" && add.identity == server.name,
            PlanAction::Update(update) => update.kind == "mcp" && update.identity == server.name,
            _ => false,
        });
        if !changed_by_sync {
            continue;
        }
        if parsed.mcp.iter().any(|m| m.native_name == server.name) {
            upsert_mcp_binding(
                pool,
                &HarnessMcpBinding {
                    id: Uuid::new_v4(),
                    harness_installation_id: install.id,
                    mcp_server_id: server.id,
                    native_name: server.name.clone(),
                    native_config: serde_json::to_value(server).map_err(|e| e.to_string())?,
                    managed: true,
                },
            )
            .await
            .map_err(|e| e.to_string())?;
        }
    }
    for skill in &desired.skills {
        let changed_by_sync = plan.actions.iter().any(|action| match action {
            PlanAction::Add(add) => {
                add.kind == "skill"
                    && (add.identity == skill.canonical_path || add.identity == skill.name)
            }
            PlanAction::Update(update) => {
                update.kind == "skill"
                    && (update.identity == skill.canonical_path || update.identity == skill.name)
            }
            _ => false,
        });
        if !changed_by_sync {
            continue;
        }
        if let Some(actual) = parsed
            .skills
            .iter()
            .find(|s| s.path == skill.canonical_path || s.name == skill.name)
        {
            upsert_skill_binding(
                pool,
                &HarnessSkillBinding {
                    id: Uuid::new_v4(),
                    harness_installation_id: install.id,
                    skill_id: skill.id,
                    target_path: actual.path.clone(),
                    binding_type: if actual.symlinked {
                        BindingType::Symlink
                    } else {
                        BindingType::Copy
                    },
                    managed: true,
                    status: "active".into(),
                },
            )
            .await
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub(crate) fn plan_hash(
    plan: &ReconciliationPlan,
    native_plan: &NativePlan,
) -> Result<String, String> {
    // Native changes are also used for previews, hashes, and audit records.
    // Adapters still retain the complete bytes for the actual write, but the
    // representation that leaves this process must not carry an API key from
    // an existing config file.
    let safe_native_plan = secret_free_native_plan(native_plan);
    let bytes = serde_json::to_vec(&(plan, safe_native_plan)).map_err(|e| e.to_string())?;
    Ok(crate::drift::sha256_hex_bytes(&bytes))
}

pub(crate) fn secret_free_native_plan(native_plan: &NativePlan) -> NativePlan {
    let mut safe = native_plan.clone();
    safe.changes = native_plan
        .changes
        .iter()
        .map(|change| chm_harness_sdk::adapter::types::NativeChange {
            file_path: change.file_path.clone(),
            before: change
                .before
                .as_deref()
                .map(|content| redact_native_content(&change.file_path, content)),
            after: change
                .after
                .as_deref()
                .map(|content| redact_native_content(&change.file_path, content)),
        })
        .collect();
    safe
}

fn redact_native_content(path: &str, content: &str) -> String {
    let extension = std::path::Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(extension.as_str(), "json" | "jsonc") {
        let parsed = if extension == "jsonc" {
            serde_json::from_reader::<_, serde_json::Value>(
                json_comments::StripComments::new(content.as_bytes()),
            )
        } else {
            serde_json::from_str(content)
        };
        if let Ok(mut value) = parsed {
            redact_json_value(&mut value);
            return serde_json::to_string_pretty(&value).unwrap_or_else(|_| "<redacted>".into());
        }
    }
    if matches!(extension.as_str(), "yaml" | "yml")
        && let Ok(mut value) = serde_yaml::from_str::<serde_yaml::Value>(content)
    {
        redact_yaml_value(&mut value);
        return serde_yaml::to_string(&value).unwrap_or_else(|_| "<redacted>\n".into());
    }
    redact_assignment_lines(content)
}

fn redact_json_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(object) => {
            for (key, child) in object.iter_mut() {
                if is_secret_field(key) {
                    *child = serde_json::Value::String("<redacted>".into());
                } else {
                    redact_json_value(child);
                }
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                redact_json_value(value);
            }
        }
        _ => {}
    }
}

fn redact_yaml_value(value: &mut serde_yaml::Value) {
    match value {
        serde_yaml::Value::Mapping(mapping) => {
            for (key, child) in mapping.iter_mut() {
                if key
                    .as_str()
                    .is_some_and(is_secret_field)
                {
                    *child = serde_yaml::Value::String("<redacted>".into());
                } else {
                    redact_yaml_value(child);
                }
            }
        }
        serde_yaml::Value::Sequence(values) => {
            for value in values {
                redact_yaml_value(value);
            }
        }
        _ => {}
    }
}

fn redact_assignment_lines(content: &str) -> String {
    let lines = content
        .lines()
        .map(|line| {
            let Some((index, separator)) = line
                .char_indices()
                .find_map(|(index, character)| matches!(character, '=' | ':').then_some((index, character)))
            else {
                return line.to_string();
            };
            let key = line[..index]
                .trim()
                .trim_start_matches('-')
                .trim()
                .trim_matches(['"', '\'']);
            if is_secret_field(key) {
                format!("{}{} \"<redacted>\"", &line[..index], separator)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>();
    let mut output = lines.join("\n");
    if content.ends_with('\n') {
        output.push('\n');
    }
    output
}

fn is_secret_field(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "secret",
        "token",
        "password",
        "passwd",
        "api_key",
        "apikey",
        "access_key",
        "private_key",
        "authorization",
        "cookie",
    ]
    .iter()
    .any(|needle| key.contains(needle))
}

pub(crate) fn validate_apply_request(
    expected_plan_hash: Option<&str>,
    current_plan_hash: &str,
    writable_changes: usize,
    has_blockers: bool,
    has_route_blockers: bool,
    force: bool,
) -> Result<(), String> {
    if let Some(expected) = expected_plan_hash
        && expected != current_plan_hash
    {
        return Err(
            "preview is stale: the library or harness changed; refresh before applying".into(),
        );
    }
    if has_route_blockers {
        return Err(
            "preview contains an incompatible provider route; fix its provider, endpoint, protocol, or credential before applying"
                .into(),
        );
    }
    if has_blockers && !force {
        return Err(
            "preview contains conflicts or unsupported changes; review them or enable Force".into(),
        );
    }
    if writable_changes == 0 {
        return Err("nothing to apply: preview contains no writable changes".into());
    }
    Ok(())
}

pub(crate) fn action_views(plan: &ReconciliationPlan) -> Vec<ActionView> {
    plan.actions
        .iter()
        .map(|a| match a {
            PlanAction::Add(x) => ActionView {
                kind: x.kind.clone(),
                identity: x.identity.clone(),
                action: "add".into(),
            },
            PlanAction::Update(x) => ActionView {
                kind: x.kind.clone(),
                identity: x.identity.clone(),
                action: "update".into(),
            },
            PlanAction::Remove(x) => ActionView {
                kind: x.kind.clone(),
                identity: x.identity.clone(),
                action: "remove".into(),
            },
            PlanAction::Unchanged(x) => ActionView {
                kind: x.kind.clone(),
                identity: x.identity.clone(),
                action: "unchanged".into(),
            },
            PlanAction::Conflict(x) => ActionView {
                kind: x.kind.clone(),
                identity: x.identity.clone(),
                action: "conflict".into(),
            },
            PlanAction::Unsupported(x) => ActionView {
                kind: x.kind.clone(),
                identity: x.identity.clone(),
                action: "unsupported".into(),
            },
            PlanAction::NoOp(x) => ActionView {
                kind: "noop".into(),
                identity: x.clone(),
                action: "noop".into(),
            },
        })
        .collect()
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

fn activity_field_label(field: &str) -> &str {
    match field {
        "context_window" => "context window",
        "max_input" => "max input",
        "max_output" => "max output",
        "display_name" => "display name",
        "capabilities" => "capabilities",
        other => other,
    }
}

fn activity_json_value(value: Option<&serde_json::Value>) -> String {
    match value {
        Some(value) if value.is_null() => "unset".into(),
        Some(value) if value.as_i64().is_some() => {
            value.as_i64().unwrap_or_default().to_string()
        }
        Some(value) if value.as_str().is_some() => {
            format!("\"{}\"", activity_value(value.as_str().unwrap_or_default()))
        }
        _ => "changed".into(),
    }
}

fn activity_model_label(id: &str, display_name: Option<&str>) -> String {
    let id = activity_value(id);
    let display = display_name
        .map(activity_value)
        .filter(|value| !value.is_empty() && !value.eq_ignore_ascii_case(&id));
    display
        .map(|name| format!("{id} ({name})"))
        .unwrap_or(id)
}

fn activity_provider_suffix(provider: Option<&str>) -> String {
    provider
        .map(|value| format!(" via {}", activity_value(value)))
        .unwrap_or_default()
}

/// Render a safe, human-readable description of the resource actions that a
/// successful library-to-harness sync applied. Only identities, provider
/// labels, and model metadata are included; native payloads and credentials
/// are never rendered into the audit summary.
pub(crate) fn activity_summary(
    harness_type: &str,
    plan: &ReconciliationPlan,
) -> String {
    let harness = activity_harness_label(harness_type);
    let mut details = Vec::new();
    for action in &plan.actions {
        match action {
            PlanAction::Add(add) if add.kind == "model" => {
                let id = add
                    .payload
                    .get("remote_model_id")
                    .and_then(|value| value.as_str())
                    .unwrap_or(&add.identity);
                let display = add.payload.get("display_name").and_then(|value| value.as_str());
                details.push(format!(
                    "Added model {}{}",
                    activity_model_label(id, display),
                    activity_provider_suffix(add.native_provider_id.as_deref())
                ));
            }
            PlanAction::Update(update) if update.kind == "model" => {
                let display = update
                    .current
                    .get("display_name")
                    .and_then(|value| value.as_str());
                let mut changes = Vec::new();
                for field in &update.changed_fields {
                    let change = match field.as_str() {
                        "display_name" => format!(
                            "display name {} → {}",
                            activity_json_value(update.current.get("display_name")),
                            activity_json_value(update.desired.get("display_name"))
                        ),
                        "context_window" => format!(
                            "context window {} → {}",
                            activity_json_value(update.current.get("context_window")),
                            activity_json_value(update.desired.get("context_window"))
                        ),
                        "max_input" => format!(
                            "max input {} → {}",
                            activity_json_value(update.current.get("max_input")),
                            activity_json_value(update.desired.get("max_input"))
                        ),
                        "max_output" => format!(
                            "max output {} → {}",
                            activity_json_value(update.current.get("max_output")),
                            activity_json_value(update.desired.get("max_output"))
                        ),
                        other => activity_field_label(other).to_string(),
                    };
                    changes.push(change);
                }
                let suffix = if changes.is_empty() {
                    String::new()
                } else {
                    format!(": {}", changes.join(", "))
                };
                details.push(format!(
                    "Updated model {}{}{suffix}",
                    activity_model_label(&update.identity, display),
                    activity_provider_suffix(update.native_provider_id.as_deref())
                ));
            }
            PlanAction::Remove(remove) if remove.kind == "model" => {
                details.push(format!(
                    "Deleted model {}{}",
                    activity_value(&remove.identity),
                    activity_provider_suffix(remove.native_provider_id.as_deref())
                ));
            }
            PlanAction::Add(add) if add.kind == "mcp" => details.push(format!(
                "Added MCP server {}",
                activity_value(&add.identity)
            )),
            PlanAction::Update(update) if update.kind == "mcp" => details.push(format!(
                "Updated MCP server {}",
                activity_value(&update.identity)
            )),
            PlanAction::Remove(remove) if remove.kind == "mcp" => details.push(format!(
                "Deleted MCP server {}",
                activity_value(&remove.identity)
            )),
            PlanAction::Add(add) if add.kind == "skill" => details.push(format!(
                "Linked skill {}",
                activity_value(&add.identity)
            )),
            PlanAction::Update(update) if update.kind == "skill" => details.push(format!(
                "Updated skill {}",
                activity_value(&update.identity)
            )),
            PlanAction::Remove(remove) if remove.kind == "skill" => details.push(format!(
                "Unlinked skill {}",
                activity_value(&remove.identity)
            )),
            _ => {}
        }
    }
    if details.is_empty() {
        return format!("{harness}: no resource details recorded");
    }
    if details.len() > 8 {
        let remaining = details.len() - 8;
        details.truncate(8);
        details.push(format!("{remaining} more resource change(s)"));
    }
    format!("{harness}: {}", details.join("; "))
}

/// Render the user-facing preview from the same plan representation used by
/// both regular library sync and configuration-set sync. Keeping this seam in
/// the sync module prevents the two preview commands from drifting in what
/// they expose (especially blockers and writable file counts).
pub(crate) fn preview_report(
    plan: &ReconciliationPlan,
    native_plan: &NativePlan,
) -> Result<PreviewReport, String> {
    let actions = action_views(plan);
    let safe_native_plan = secret_free_native_plan(native_plan);
    let files = safe_native_plan
        .changes
        .iter()
        .map(|change| FilePreview {
            path: change.file_path.clone(),
            before: change.before.clone(),
            after: change.after.clone(),
        })
        .collect();
    let warnings = native_plan.warnings.clone();
    let has_blockers = actions
        .iter()
        .any(|action| action.action == "conflict" || action.action == "unsupported")
        || !warnings.is_empty();
    let route_blockers = plan
        .actions
        .iter()
        .filter_map(|action| match action {
            PlanAction::Unsupported(action) if action.kind == "provider-route" => {
                Some(RouteBlockerView {
                    provider_id: action.identity.clone(),
                    model_ids: action.model_ids.clone(),
                    reason: action.reason.clone(),
                })
            }
            _ => None,
        })
        .collect();
    Ok(PreviewReport {
        summary: plan.summary(),
        actions,
        files,
        plan_hash: plan_hash(plan, native_plan)?,
        writable_changes: native_plan.changes.len(),
        protected_changes: native_plan.protected_changes.len(),
        has_blockers,
        warnings,
        route_blockers,
    })
}

pub async fn build_native_plan(
    pool: &Pool<Sqlite>,
    installation_id: &str,
    mode: &Mode,
) -> Result<
    (
        HarnessInstallation,
        Box<dyn HarnessAdapter>,
        ReconciliationPlan,
        NativePlan,
    ),
    String,
> {
    build_native_plan_scoped(pool, installation_id, mode, None).await
}

pub async fn build_native_plan_scoped(
    pool: &Pool<Sqlite>,
    installation_id: &str,
    mode: &Mode,
    selection: Option<&SyncSelection>,
) -> Result<
    (
        HarnessInstallation,
        Box<dyn HarnessAdapter>,
        ReconciliationPlan,
        NativePlan,
    ),
    String,
> {
    let desired = desired_state(pool, selection).await?;
    build_native_plan_for_desired(pool, installation_id, mode, desired).await
}

/// Build a native plan from an explicit desired state. Configuration sets use
/// this same path so their preview and apply semantics cannot drift from the
/// normal library sync flow.
pub async fn build_native_plan_for_desired(
    pool: &Pool<Sqlite>,
    installation_id: &str,
    mode: &Mode,
    desired: DesiredState,
) -> Result<
    (
        HarnessInstallation,
        Box<dyn HarnessAdapter>,
        ReconciliationPlan,
        NativePlan,
    ),
    String,
> {
    let inst = crate::commands::find_installation(pool, installation_id).await?;
    let adapter = adapter_for(inst.harness_type.as_str()).ok_or("no adapter")?;
    let parsed = adapter.read_state(&inst).map_err(|e| e.to_string())?;
    let actual = ActualState {
        routes: parsed.models.clone(),
        mcp: parsed.mcp.clone(),
        skills: parsed.skills.clone(),
        managed_flags: managed_flags_for(pool, &inst, &parsed).await?,
    };
    let caps = adapter.capabilities();
    let plan = chm_reconciliation::engine::reconcile_with_capabilities(
        &desired, &actual, *mode, &caps,
    )
    .map_err(|e| e.to_string())?;
    let plan = filter_unsupported(plan, &caps);
    let native_plan = adapter.plan(&plan, &inst).map_err(|e| e.to_string())?;
    Ok((inst, adapter, plan, native_plan))
}

#[tauri::command]
pub async fn sync_preview(
    state: State<'_, AppState>,
    installation_id: String,
    mode: String,
    selection: Option<SyncSelection>,
) -> Result<PreviewReport, String> {
    let m = effective_mode(&mode, selection.as_ref())?;
    let (_, _, plan, native_plan) =
        build_native_plan_scoped(&state.pool, &installation_id, &m, selection.as_ref()).await?;
    preview_report(&plan, &native_plan)
}

pub async fn execute_sync(
    pool: &Pool<Sqlite>,
    installation_id: &str,
    mode: &Mode,
    force: bool,
) -> Result<ApplyReport, String> {
    let secrets = chm_secrets::default_store();
    execute_sync_with_plan_using_secrets(
        pool,
        installation_id,
        mode,
        force,
        None,
        None,
        &*secrets,
    )
    .await
}

pub async fn execute_sync_with_plan(
    pool: &Pool<Sqlite>,
    installation_id: &str,
    mode: &Mode,
    force: bool,
    expected_plan_hash: Option<&str>,
    selection: Option<&SyncSelection>,
) -> Result<ApplyReport, String> {
    let secrets = chm_secrets::default_store();
    execute_sync_with_plan_using_secrets(
        pool,
        installation_id,
        mode,
        force,
        expected_plan_hash,
        selection,
        &*secrets,
    )
    .await
}

async fn execute_sync_with_plan_using_secrets(
    pool: &Pool<Sqlite>,
    installation_id: &str,
    mode: &Mode,
    force: bool,
    expected_plan_hash: Option<&str>,
    selection: Option<&SyncSelection>,
    secrets: &dyn SecretStore,
) -> Result<ApplyReport, String> {
    let desired = desired_state(pool, selection).await?;
    execute_desired_with_plan_using_secrets(
        pool,
        installation_id,
        mode,
        force,
        expected_plan_hash,
        selection,
        desired,
        secrets,
    )
    .await
}

/// Execute a validated native plan built from an explicit desired state.
/// Configuration sets call this instead of maintaining a second transaction
/// implementation, keeping backup, snapshot, validation, rollback, and
/// ownership semantics identical to normal sync.
pub async fn execute_desired_with_plan(
    pool: &Pool<Sqlite>,
    installation_id: &str,
    mode: &Mode,
    force: bool,
    expected_plan_hash: Option<&str>,
    selection: Option<&SyncSelection>,
    desired: DesiredState,
) -> Result<ApplyReport, String> {
    let secrets = chm_secrets::default_store();
    execute_desired_with_plan_using_secrets(
        pool,
        installation_id,
        mode,
        force,
        expected_plan_hash,
        selection,
        desired,
        &*secrets,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn execute_desired_with_plan_using_secrets(
    pool: &Pool<Sqlite>,
    installation_id: &str,
    mode: &Mode,
    force: bool,
    expected_plan_hash: Option<&str>,
    selection: Option<&SyncSelection>,
    desired: DesiredState,
    secrets: &dyn SecretStore,
) -> Result<ApplyReport, String> {
    if selection.is_some() && matches!(mode, Mode::ReplaceManaged) {
        return Err(
            "selection-scoped sync only supports Append; choose the full library scope for Replace Managed"
                .into(),
        );
    }
    let effective_mode = if selection.is_some() {
        Mode::Append
    } else {
        *mode
    };
    let (inst, adapter, plan, native_plan) =
        build_native_plan_for_desired(pool, installation_id, &effective_mode, desired.clone())
            .await?;
    let activity = activity_summary(inst.harness_type.as_str(), &plan);
    let current_hash = plan_hash(&plan, &native_plan)?;
    let blockers = plan
        .actions
        .iter()
        .any(|action| matches!(action, PlanAction::Conflict(_) | PlanAction::Unsupported(_)))
        || !native_plan.warnings.is_empty();
    let route_blockers = plan.actions.iter().any(
        |action| matches!(action, PlanAction::Unsupported(action) if action.kind == "provider-route"),
    );
    // Protected credential writes are real apply work even when a native
    // adapter only needs to update an auth file. The preview keeps the two
    // counts separate for the UI, while apply validation considers both.
    let writable_changes = native_plan.changes.len() + native_plan.protected_changes.len();
    validate_apply_request(
        expected_plan_hash,
        &current_hash,
        writable_changes,
        blockers,
        route_blockers,
        force,
    )?;

    // Resolve every credential and capture every protected auth target before
    // beginning the transaction or mutating any ordinary harness file. This
    // keeps a missing key from producing a half-applied sync.
    let credential_refs = desired
        .provider_routes
        .iter()
        .filter_map(|bundle| match &bundle.credential {
            CredentialRequirement::None => None,
            CredentialRequirement::Secret { credential_ref, .. } => {
                Some((credential_ref.id, credential_ref.clone()))
            }
        })
        .collect::<std::collections::HashMap<_, CredentialRef>>();
    let mut prepared_credentials = credential_deployment::preflight(
        &native_plan.protected_changes,
        &credential_refs,
        secrets,
    )
    .map_err(|error| error.to_string())?;

    let audit_native_plan = secret_free_native_plan(&native_plan);
    let tx = begin_transaction(pool, TransactionType::Sync, serde_json::json!(audit_native_plan))
        .await
        .map_err(|e| e.to_string())?;
    let mut backups = Vec::new();
    let mut protected_applied: Vec<AppliedFile> = Vec::new();
    let mut result = ApplyReport {
        summary: String::new(),
        files_written: Vec::new(),
        links_created: Vec::new(),
        transaction_id: tx.id.to_string(),
        validation: ValidationReport {
            ok: true,
            errors: vec![],
        },
    };

    // backups first (all-or-nothing before any mutation)
    for change in &native_plan.changes {
        match backup_file(std::path::Path::new(&change.file_path)) {
            Ok(b) => backups.push((change.file_path.clone(), b)),
            Err(e) => {
                let msg = format!("backup failed before write: {e}");
                rollback_all(
                    pool,
                    tx.id,
                    &*adapter,
                    &inst,
                    &native_plan,
                    &backups,
                    &mut protected_applied,
                    std::slice::from_ref(&msg),
                )
                .await?;
                return Err(msg);
            }
        }
    }

    let apply_outcome: Result<ApplyResult, String> = (async {
        credential_deployment::verify_preflight(&prepared_credentials)
            .map_err(|error| error.to_string())?;
        let apply_result: Result<ApplyResult, String> = adapter
            .apply(&inst, &native_plan)
            .map_err(|e| e.to_string());
        let apply_result = apply_result?;
        credential_deployment::rebase_for_native_changes(
            &mut prepared_credentials,
            &native_plan.changes,
        )
        .map_err(|error| error.to_string())?;
        protected_applied = credential_deployment::apply(&prepared_credentials)
            .map_err(|error| error.to_string())?;
        for (file, backup) in &backups {
            let before = std::fs::read_to_string(backup).ok();
            let after = std::fs::read_to_string(file).ok();
            let hash = crate::drift::sha256_hex;
            add_snapshot(
                pool,
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
            .map_err(|e| e.to_string())?;
            let _ = (&before, &after);
        }
        Ok(apply_result)
    })
    .await;

    match apply_outcome {
        Ok(apply_result) => {
            result.files_written = apply_result.files_written;
            for protected in &protected_applied {
                let path = protected.path().display().to_string();
                if !result.files_written.contains(&path) {
                    result.files_written.push(path);
                }
            }
            result.links_created = apply_result.links_created;
            match adapter.validate(&inst) {
                Ok(v) => {
                    let ok = v.ok;
                    result.validation = v;
                    if ok {
                        let desired_after = desired.clone();
                        let parsed_after = match adapter.read_state(&inst) {
                            Ok(parsed) => parsed,
                            Err(error) => {
                                let message = error.to_string();
                                rollback_all(
                                    pool,
                                    tx.id,
                                    &*adapter,
                                    &inst,
                                    &native_plan,
                                    &backups,
                                    &mut protected_applied,
                                    std::slice::from_ref(&message),
                                )
                                .await?;
                                return Err(format!(
                                    "sync validation passed but the resulting state could not be read; rolled back: {message}"
                                ));
                            }
                        };
                        if let Err(error) =
                            record_bindings(pool, &inst, &desired_after, &plan, &parsed_after)
                                .await
                        {
                            rollback_all(
                                pool,
                                tx.id,
                                &*adapter,
                                &inst,
                                &native_plan,
                                &backups,
                                &mut protected_applied,
                                std::slice::from_ref(&error),
                            )
                            .await?;
                            return Err(format!(
                                "sync succeeded on disk but ownership could not be recorded; rolled back: {error}"
                            ));
                        }
                        finish_transaction(
                            pool,
                            tx.id,
                            TransactionStatus::Succeeded,
                            Some(activity.clone()),
                            None,
                        )
                        .await
                        .map_err(|e| e.to_string())?;
                    } else {
                        rollback_all(
                            pool,
                            tx.id,
                            &*adapter,
                            &inst,
                            &native_plan,
                            &backups,
                            &mut protected_applied,
                            &result.validation.errors,
                        )
                        .await?;
                        return Err(format!(
                            "validation failed after apply; rolled back: {:?}",
                            result.validation.errors
                        ));
                    }
                }
                Err(e) => {
                    rollback_all(
                        pool,
                        tx.id,
                        &*adapter,
                        &inst,
                        &native_plan,
                        &backups,
                        &mut protected_applied,
                        &[e.to_string()],
                    )
                    .await?;
                    return Err(format!("apply failed; rolled back: {e}"));
                }
            }
        }
        Err(e) => {
            rollback_all(
                pool,
                tx.id,
                &*adapter,
                &inst,
                &native_plan,
                &backups,
                &mut protected_applied,
                std::slice::from_ref(&e),
            )
            .await?;
            return Err(e);
        }
    }

    result.summary = format!(
        "{activity}; {} file(s) written, {} link(s) created",
        result.files_written.len(),
        result.links_created.len()
    );
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
async fn rollback_all(
    pool: &Pool<Sqlite>,
    tx_id: Uuid,
    adapter: &dyn HarnessAdapter,
    inst: &HarnessInstallation,
    native_plan: &NativePlan,
    backups: &[(String, std::path::PathBuf)],
    protected_applied: &mut Vec<AppliedFile>,
    errors: &[String],
) -> Result<(), String> {
    let protected_error = credential_deployment::rollback(protected_applied)
        .err()
        .map(|error| error.to_string());
    let native_result = crate::services::transactions::rollback_native_transaction(
        pool,
        tx_id,
        adapter,
        inst,
        native_plan,
        backups,
        errors,
    )
    .await;
    match (protected_error, native_result) {
        (None, result) => result,
        (Some(protected), Ok(())) => Err(format!(
            "{protected}; native harness changes were rolled back"
        )),
        (Some(protected), Err(native)) => Err(format!(
            "{native}; protected credential rollback also failed: {protected}"
        )),
    }
}

#[tauri::command]
pub async fn sync_apply(
    state: State<'_, AppState>,
    installation_id: String,
    mode: String,
    force: bool,
    plan_hash: String,
    selection: Option<SyncSelection>,
) -> Result<ApplyReport, String> {
    execute_sync_with_plan_using_secrets(
        &state.pool,
        &installation_id,
        &parse_mode(&mode),
        force,
        Some(&plan_hash),
        selection.as_ref(),
        &*state.secrets,
    )
    .await
}

/// Syncs ONE canonical MCP server into a harness's native config using the
/// full sync machinery (backups, snapshots, verify, rollback).
pub async fn bind_mcp_sync(
    pool: &Pool<Sqlite>,
    inst: &HarnessInstallation,
    server: &chm_core::domain::mcp::McpServer,
) -> Result<(), String> {
    let desired = DesiredState {
        mcp_servers: vec![server.clone()],
        ..Default::default()
    };
    let installation_id = inst.id.to_string();
    let (_, _adapter, plan, native_plan) =
        build_native_plan_for_desired(pool, &installation_id, &Mode::Append, desired.clone())
            .await?;
    if native_plan.changes.is_empty() {
        return Ok(());
    }
    let expected_hash = plan_hash(&plan, &native_plan)?;
    execute_desired_with_plan(
        pool,
        &installation_id,
        &Mode::Append,
        false,
        Some(&expected_hash),
        None,
        desired,
    )
    .await
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::{
        SyncSelection, activity_summary, effective_mode, group_provider_routes,
        redact_native_content, secret_free_native_plan, validate_apply_request,
    };
    use chm_core::domain::credentials::{CredentialKind, CredentialRef};
    use chm_harness_sdk::adapter::plan::{
        AddAction, Mode, PlanAction, ReconciliationPlan, RemoveAction, UpdateAction,
    };
    use chm_core::domain::models::ModelRoute;
    use chm_core::domain::provider::{AuthType, Protocol, Provider, ProviderEndpoint};
    use chm_harness_sdk::adapter::route::CredentialRequirement;
    use chm_harness_sdk::adapter::types::{NativeChange, NativePlan};
    use chrono::Utc;
    use uuid::Uuid;

    #[test]
    fn groups_models_by_endpoint_without_resolving_credentials() {
        let provider_id = Uuid::new_v4();
        let endpoint_id = Uuid::new_v4();
        let now = Utc::now();
        let provider = Provider {
            id: provider_id,
            name: "yolo-auto".into(),
            display_name: "Yolo-Auto".into(),
            enabled: true,
            notes: None,
            created_at: now,
            updated_at: now,
        };
        let credential = CredentialRef {
            id: Uuid::new_v4(),
            kind: CredentialKind::Keychain,
            reference: "coding-harness-manager/providers/yolo-auto".into(),
            created_at: now,
            updated_at: now,
        };
        let endpoint = ProviderEndpoint {
            id: endpoint_id,
            provider_id,
            name: "API".into(),
            base_url: "https://yolo-auto.example/v1".into(),
            protocol: Protocol::OpenAiChatCompletions,
            discovery_path: Some("/models".into()),
            auth_type: AuthType::BearerToken,
            credential_ref: Some(credential),
            headers: Default::default(),
            enabled: true,
            created_at: now,
            updated_at: now,
        };
        let routes = ["qwen-a", "qwen-b"]
            .into_iter()
            .map(|id| {
                let mut route = ModelRoute::new(
                    id.into(),
                    id.into(),
                    None,
                    serde_json::json!({}),
                    serde_json::json!({}),
                );
                route.endpoint_id = endpoint_id;
                route
            })
            .collect::<Vec<_>>();

        let bundles = group_provider_routes(&routes, &[provider], &[endpoint]).unwrap();

        assert_eq!(bundles.len(), 1);
        assert_eq!(bundles[0].provider_id, "yolo-auto");
        assert_eq!(bundles[0].models.len(), 2);
        assert!(matches!(
            &bundles[0].credential,
            CredentialRequirement::Secret { credential_ref, .. }
                if credential_ref.kind == CredentialKind::Keychain
        ));
    }

    #[test]
    fn endpoint_without_provider_or_credential_is_an_error() {
        let mut route = ModelRoute::new(
            "qwen".into(),
            "Qwen".into(),
            None,
            serde_json::json!({}),
            serde_json::json!({}),
        );
        route.endpoint_id = Uuid::new_v4();
        assert!(group_provider_routes(&[route], &[], &[]).is_err());

        let provider_id = Uuid::new_v4();
        let endpoint_id = Uuid::new_v4();
        let now = Utc::now();
        let provider = Provider {
            id: provider_id,
            name: "missing-secret".into(),
            display_name: "Missing Secret".into(),
            enabled: true,
            notes: None,
            created_at: now,
            updated_at: now,
        };
        let endpoint = ProviderEndpoint {
            id: endpoint_id,
            provider_id,
            name: "API".into(),
            base_url: "https://example.test/v1".into(),
            protocol: Protocol::OpenAiChatCompletions,
            discovery_path: None,
            auth_type: AuthType::BearerToken,
            credential_ref: None,
            headers: Default::default(),
            enabled: true,
            created_at: now,
            updated_at: now,
        };
        let mut route = ModelRoute::new(
            "qwen".into(),
            "Qwen".into(),
            None,
            serde_json::json!({}),
            serde_json::json!({}),
        );
        route.endpoint_id = endpoint_id;
        let error = group_provider_routes(&[route], &[provider], &[endpoint]).unwrap_err();
        assert!(error.contains("has no credential"));
    }

    #[test]
    fn stale_preview_is_rejected_before_writes() {
        let error =
            validate_apply_request(Some("old"), "new", 1, false, false, false).unwrap_err();
        assert!(error.contains("stale"));
    }

    #[test]
    fn no_op_preview_cannot_apply() {
        let error =
            validate_apply_request(Some("same"), "same", 0, false, false, false).unwrap_err();
        assert!(error.contains("no writable"));
    }

    #[test]
    fn blockers_need_force_and_force_is_explicit() {
        let error =
            validate_apply_request(Some("same"), "same", 1, true, false, false).unwrap_err();
        assert!(error.contains("conflicts"));
        assert!(validate_apply_request(Some("same"), "same", 1, true, false, true).is_ok());
    }

    #[test]
    fn provider_route_blockers_cannot_be_forced() {
        let error =
            validate_apply_request(Some("same"), "same", 1, true, true, true).unwrap_err();
        assert!(error.contains("provider route"));
    }

    #[test]
    fn selection_scope_rejects_replace_managed_instead_of_silently_downgrading() {
        let selection = SyncSelection {
            model_ids: vec!["route".into()],
            ..Default::default()
        };
        let error = effective_mode("replaceManaged", Some(&selection)).unwrap_err();
        assert!(error.contains("selection-scoped"));
        assert!(matches!(
            effective_mode("append", Some(&selection)),
            Ok(Mode::Append)
        ));
    }

    #[test]
    fn activity_summary_names_models_providers_and_changed_fields() {
        let plan = ReconciliationPlan {
            actions: vec![
                PlanAction::Add(AddAction {
                    kind: "model".into(),
                    identity: "qwen3.8-27b".into(),
                    payload: serde_json::json!({
                        "remote_model_id": "qwen3.8-27b",
                        "display_name": "Qwen 3.8 27B"
                    }),
                    native_provider_id: Some("Yolo-Auto".into()),
                }),
                PlanAction::Update(UpdateAction {
                    kind: "model".into(),
                    identity: "glm-5.2".into(),
                    changed_fields: vec!["context_window".into()],
                    desired: serde_json::json!({"context_window": 200000}),
                    current: serde_json::json!({
                        "display_name": "GLM 5.2",
                        "context_window": 128000
                    }),
                    native_provider_id: Some("pattycode".into()),
                }),
                PlanAction::Remove(RemoveAction {
                    kind: "model".into(),
                    identity: "old-model".into(),
                    native_provider_id: Some("pattycode".into()),
                }),
            ],
        };
        let summary = activity_summary("pi", &plan);
        assert!(summary.contains("Pi: Added model qwen3.8-27b (Qwen 3.8 27B) via Yolo-Auto"));
        assert!(summary.contains("Updated model glm-5.2 (GLM 5.2) via pattycode: context window 128000 → 200000"));
        assert!(summary.contains("Deleted model old-model via pattycode"));
    }

    #[test]
    fn native_plan_preview_redacts_existing_credentials() {
        let before = r#"{"provider":{"apiKey":"sk-live-secret","baseUrl":"https://example.test/v1"}}"#;
        let after = r#"{"provider":{"apiKey":"sk-new-secret","baseUrl":"https://example.test/v1"}}"#;
        let plan = NativePlan {
            changes: vec![NativeChange {
                file_path: "/tmp/opencode.json".into(),
                before: Some(before.into()),
                after: Some(after.into()),
            }],
            ..Default::default()
        };
        let safe = secret_free_native_plan(&plan);
        let serialized = serde_json::to_string(&safe).unwrap();
        assert!(!serialized.contains("sk-live-secret"));
        assert!(!serialized.contains("sk-new-secret"));
        assert!(serialized.contains("<redacted>"));
        assert!(redact_native_content("/tmp/settings.yaml", "api_key: sk-secret\n")
            .contains("<redacted>"));
    }
}
