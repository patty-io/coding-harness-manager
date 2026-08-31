//! Model reconciliation: desired routes vs actual harness models.

use chm_core::domain::models::ModelRoute;
use chm_harness_sdk::adapter::types::HarnessModel;
use std::collections::HashMap;

use crate::plan::{
    AddAction, ConflictAction, Mode, PlanAction, RemoveAction, UnchangedAction, UpdateAction,
};

const UPDATABLE_FIELDS: &[&str] = &[
    "context_window",
    "max_input",
    "max_output",
    "display_name",
    "capabilities",
];

pub fn reconcile_models(
    desired: &[ModelRoute],
    actual: &[HarnessModel],
    mode: Mode,
    managed: &HashMap<String, bool>,
) -> Vec<PlanAction> {
    let mut actions = Vec::new();
    // Native identity is provider-scoped for formats such as Pi. Keeping the
    // provider in the lookup prevents one provider's `qwen` from shadowing a
    // second provider's `qwen`.
    let actual_by_identity: HashMap<(Option<&str>, &str), &HarnessModel> =
        actual.iter().map(|m| (native_identity(m), m)).collect();

    for d in desired {
        let native_id = d.remote_model_id.as_str();
        let provider = route_provider(d);
        let matching = actual_by_identity
            .get(&(provider, native_id))
            .copied()
            .or_else(|| {
                // Some adapters use a stable native provider id (for
                // example Continue's `openai`) while the library route uses
                // the registry slug. The endpoint base URL is the portable
                // identity in that case, so use it as a safe fallback.
                actual.iter().find(|a| route_identity_matches(a, d))
            });
        match matching {
            None => {
                let alias_hit = actual.iter().find(|a| {
                    a.route.remote_model_id == d.remote_model_id
                        && a.native_id != native_id
                        && native_identity(a).0 == provider
                });
                if let Some(hit) = alias_hit {
                    actions.push(PlanAction::Conflict(ConflictAction {
                        kind: "model".into(),
                        identity: native_id.into(),
                        reason: format!("exists in harness as native id '{}'", hit.native_id),
                        desired: serde_json::json!(d),
                        current: serde_json::json!(hit.route),
                    }));
                } else {
                    actions.push(PlanAction::Add(AddAction {
                        kind: "model".into(),
                        identity: native_id.into(),
                        payload: serde_json::json!({
                            "native_provider_id": provider,
                            "remote_model_id": d.remote_model_id,
                            "display_name": d.display_name,
                            "context_window": d.context_window,
                            "max_input": d.max_input,
                            "max_output": d.max_output,
                            "capabilities": d.capabilities,
                            "overrides": d.overrides,
                            "base_url": d.overrides.get("base_url").cloned(),
                            "protocol": d.overrides.get("protocol").cloned(),
                            "env_key": d.overrides.get("env_key").cloned(),
                            "api_key_env": d.overrides.get("api_key_env").cloned(),
                            "credential_ref_id": d
                                .overrides
                                .get("native_provider_config")
                                .and_then(|config| config.get("credential_ref_id"))
                                .cloned(),
                            "credential_kind": d
                                .overrides
                                .get("native_provider_config")
                                .and_then(|config| config.get("credential_kind"))
                                .cloned(),
                            "credential_reference": d
                                .overrides
                                .get("native_provider_config")
                                .and_then(|config| config.get("credential_reference"))
                                .cloned(),
                        }),
                        native_provider_id: provider.map(str::to_string),
                    }));
                }
            }
            Some(a) => {
                let changed: Vec<String> = UPDATABLE_FIELDS
                    .iter()
                    .filter(|f| field_differs(f, &a.route, d))
                    .map(|f| f.to_string())
                    .collect();
                if changed.is_empty() {
                    actions.push(PlanAction::Unchanged(UnchangedAction {
                        kind: "model".into(),
                        identity: native_id.into(),
                    }));
                } else {
                    actions.push(PlanAction::Update(UpdateAction {
                        kind: "model".into(),
                        identity: native_id.into(),
                        changed_fields: changed,
                        desired: serde_json::json!(d),
                        current: serde_json::json!(a.route),
                        native_provider_id: native_identity(a).0.map(str::to_string),
                    }));
                }
            }
        }
    }

    if matches!(mode, Mode::ReplaceManaged) {
        for a in actual {
            let key = format!("route:{}:{}", a.route.endpoint_id, a.native_id);
            let is_managed = managed.get(&key).copied().unwrap_or(false);
            if !is_managed {
                continue;
            }
            let still_desired = desired.iter().any(|d| route_identity_matches(a, d));
            if !still_desired {
                actions.push(PlanAction::Remove(RemoveAction {
                    kind: "model".into(),
                    identity: a.native_id.clone(),
                    native_provider_id: native_identity(a).0.map(str::to_string),
                }));
            }
        }
    }

    actions
}

fn route_provider(route: &ModelRoute) -> Option<&str> {
    route
        .overrides
        .get("native_provider_id")
        .and_then(|v| v.as_str())
        .or_else(|| {
            // Imports retain the adapter's native overrides under `native`
            // alongside provenance. Keep those routes provider-scoped when
            // reconciling them after they have been persisted.
            route
                .overrides
                .get("native")
                .and_then(|native| native.get("native_provider_id"))
                .and_then(|v| v.as_str())
        })
}

fn native_identity(model: &HarnessModel) -> (Option<&str>, &str) {
    (route_provider(&model.route), model.native_id.as_str())
}

/// Return an endpoint base URL carried by a route's adapter metadata. The
/// metadata is deliberately non-secret and survives round-trips through the
/// database, unlike credentials or provider display names.
fn route_base_url(route: &ModelRoute) -> Option<&str> {
    route
        .overrides
        .get("base_url")
        .and_then(|v| v.as_str())
        .or_else(|| {
            route
                .overrides
                .get("native_provider_config")
                .and_then(|config| config.get("base_url"))
                .and_then(|v| v.as_str())
        })
        .or_else(|| {
            route
                .overrides
                .get("native")
                .and_then(|native| native.get("base_url"))
                .and_then(|v| v.as_str())
        })
        .or_else(|| {
            route
                .overrides
                .get("native")
                .and_then(|native| native.get("api_base"))
                .and_then(|v| v.as_str())
        })
}

fn normalize_base_url(value: &str) -> String {
    value.trim().trim_end_matches('/').to_ascii_lowercase()
}

/// Match a harness row to a desired route by native model id and provider.
/// Provider ids are preferred; matching by the same normalized endpoint URL
/// handles adapters that expose a protocol-specific provider alias.
fn route_identity_matches(actual: &HarnessModel, desired: &ModelRoute) -> bool {
    if actual.native_id != desired.remote_model_id {
        return false;
    }
    let actual_provider = route_provider(&actual.route);
    let desired_provider = route_provider(desired);
    if actual_provider == desired_provider {
        return true;
    }
    match (route_base_url(&actual.route), route_base_url(desired)) {
        (Some(actual), Some(desired)) => normalize_base_url(actual) == normalize_base_url(desired),
        _ => false,
    }
}

fn field_differs(field: &str, actual: &ModelRoute, desired: &ModelRoute) -> bool {
    match field {
        "context_window" => actual.context_window != desired.context_window,
        "max_input" => actual.max_input != desired.max_input,
        "max_output" => actual.max_output != desired.max_output,
        "display_name" => actual.display_name != desired.display_name,
        // empty desired capabilities = "leave native as-is"; only flag drift
        // when the desired side actually specifies capability metadata
        "capabilities" => {
            !desired.capabilities.is_null()
                && !desired
                    .capabilities
                    .as_object()
                    .is_some_and(|m| m.is_empty())
                && desired.capabilities != actual.capabilities
        }
        _ => false,
    }
}
