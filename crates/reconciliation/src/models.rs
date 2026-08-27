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
    let actual_by_native: HashMap<&str, &HarnessModel> =
        actual.iter().map(|m| (m.native_id.as_str(), m)).collect();

    for d in desired {
        let native_id = d.remote_model_id.as_str();
        match actual_by_native.get(native_id) {
            None => {
                let alias_hit = actual.iter().find(|a| {
                    a.route.remote_model_id == d.remote_model_id && a.native_id != native_id
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
                            "remote_model_id": d.remote_model_id,
                            "display_name": d.display_name,
                            "context_window": d.context_window,
                            "max_input": d.max_input,
                            "max_output": d.max_output,
                            "capabilities": d.capabilities,
                            "overrides": d.overrides,
                        }),
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
            let still_desired = desired.iter().any(|d| d.remote_model_id == a.native_id);
            if !still_desired {
                actions.push(PlanAction::Remove(RemoveAction {
                    kind: "model".into(),
                    identity: a.native_id.clone(),
                }));
            }
        }
    }

    actions
}

fn field_differs(field: &str, actual: &ModelRoute, desired: &ModelRoute) -> bool {
    match field {
        "context_window" => actual.context_window != desired.context_window,
        "max_input" => actual.max_input != desired.max_input,
        "max_output" => actual.max_output != desired.max_output,
        "display_name" => actual.display_name != desired.display_name,
        "capabilities" => actual.capabilities != desired.capabilities,
        _ => false,
    }
}
