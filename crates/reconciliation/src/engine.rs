//! Top-level reconciliation: desired + actual -> plan.

use chm_harness_sdk::adapter::types::HarnessCapabilities;

use crate::mcp_skills::{reconcile_mcp, reconcile_skills};
use crate::models::reconcile_models;
use crate::plan::{
    ActualState, DesiredState, Mode, PlanAction, ReconcileError, ReconciliationPlan,
    UnsupportedAction,
};

pub fn reconcile(
    desired: &DesiredState,
    actual: &ActualState,
    mode: Mode,
) -> Result<ReconciliationPlan, ReconcileError> {
    if desired
        .routes
        .iter()
        .any(|r| r.remote_model_id.trim().is_empty())
    {
        return Err(ReconcileError::InvalidDesired(
            "route with empty remote_model_id".into(),
        ));
    }
    if actual.routes.iter().any(|r| r.native_id.trim().is_empty()) {
        return Err(ReconcileError::InvalidActual(
            "actual model with empty native_id".into(),
        ));
    }
    let mut actions = Vec::new();
    actions.extend(reconcile_models(
        &desired.routes,
        &actual.routes,
        mode,
        &actual.managed_flags,
    ));
    actions.extend(reconcile_mcp(
        &desired.mcp_servers,
        &actual.mcp,
        mode,
        &actual.managed_flags,
    ));
    actions.extend(reconcile_skills(
        &desired.skills,
        &actual.skills,
        mode,
        &actual.managed_flags,
    ));
    Ok(ReconciliationPlan { actions })
}

pub fn filter_unsupported(
    plan: ReconciliationPlan,
    caps: &HarnessCapabilities,
) -> ReconciliationPlan {
    let actions = plan
        .actions
        .into_iter()
        .map(|action| {
            let kind = match &action {
                PlanAction::Add(a) => Some(a.kind.as_str()),
                PlanAction::Update(a) => Some(a.kind.as_str()),
                PlanAction::Remove(a) => Some(a.kind.as_str()),
                _ => None,
            };
            let supports = match kind {
                Some("model") => caps.supports_custom_models,
                Some("mcp") => caps.supports_mcp_global,
                Some("skill") => caps.supports_global_skills,
                _ => true,
            };
            if supports {
                action
            } else {
                let (identity, reason) = match &action {
                    PlanAction::Add(a) => (
                        a.identity.clone(),
                        "harness does not support this resource".into(),
                    ),
                    PlanAction::Update(a) => (
                        a.identity.clone(),
                        "harness does not support updates to this resource".into(),
                    ),
                    PlanAction::Remove(a) => (
                        a.identity.clone(),
                        "harness does not support this resource".into(),
                    ),
                    _ => (String::new(), String::new()),
                };
                PlanAction::Unsupported(UnsupportedAction {
                    kind: "resource".into(),
                    identity,
                    reason,
                })
            }
        })
        .collect();
    ReconciliationPlan { actions }
}
