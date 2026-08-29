//! MCP + skill reconciliation.

use chm_core::domain::mcp::McpServer;
use chm_core::domain::skills::Skill;
use chm_harness_sdk::adapter::types::{HarnessMcp, HarnessSkill};
use std::collections::HashMap;

use crate::plan::{
    AddAction, ConflictAction, Mode, PlanAction, RemoveAction, UnchangedAction, UnsupportedAction,
};

pub fn reconcile_mcp(
    desired: &[McpServer],
    actual: &[HarnessMcp],
    mode: Mode,
    managed: &HashMap<String, bool>,
) -> Vec<PlanAction> {
    let mut actions = Vec::new();
    let actual_by_name: HashMap<&str, &HarnessMcp> =
        actual.iter().map(|m| (m.native_name.as_str(), m)).collect();

    for d in desired {
        match actual_by_name.get(d.name.as_str()) {
            None => actions.push(PlanAction::Add(AddAction {
                kind: "mcp".into(),
                identity: d.name.clone(),
                payload: serde_json::json!(d),
                native_provider_id: None,
            })),
            Some(a) => {
                let changed: Vec<String> = ["command", "args", "url", "env", "transport"]
                    .iter()
                    .filter(|f| mcp_field_differs(f, &a.server, d))
                    .map(|f| f.to_string())
                    .collect();
                if changed.is_empty() {
                    actions.push(PlanAction::Unchanged(UnchangedAction {
                        kind: "mcp".into(),
                        identity: d.name.clone(),
                    }));
                } else if changed.contains(&"command".to_string())
                    && changed.contains(&"transport".to_string())
                {
                    actions.push(PlanAction::Conflict(ConflictAction {
                        kind: "mcp".into(),
                        identity: d.name.clone(),
                        reason: "same name but different command AND transport".into(),
                        desired: serde_json::json!(d),
                        current: serde_json::json!(a.server),
                    }));
                } else {
                    actions.push(PlanAction::Update(crate::plan::UpdateAction {
                        kind: "mcp".into(),
                        identity: d.name.clone(),
                        changed_fields: changed,
                        desired: serde_json::json!(d),
                        current: serde_json::json!(a.server),
                        native_provider_id: None,
                    }));
                }
            }
        }
    }

    if matches!(mode, Mode::ReplaceManaged) {
        for a in actual {
            let key = format!("mcp:{}", a.native_name);
            let is_managed = managed.get(&key).copied().unwrap_or(false);
            if !is_managed {
                continue;
            }
            let still_desired = desired.iter().any(|d| d.name == a.native_name);
            if !still_desired {
                actions.push(PlanAction::Remove(RemoveAction {
                    kind: "mcp".into(),
                    identity: a.native_name.clone(),
                    native_provider_id: None,
                }));
            }
        }
    }

    actions
}

fn mcp_field_differs(field: &str, actual: &McpServer, desired: &McpServer) -> bool {
    match field {
        "command" => actual.command != desired.command,
        "args" => actual.args != desired.args,
        "url" => actual.url != desired.url,
        "env" => actual.env != desired.env,
        "transport" => actual.transport != desired.transport,
        _ => false,
    }
}

pub fn reconcile_skills(
    desired: &[Skill],
    actual: &[HarnessSkill],
    mode: Mode,
    managed: &HashMap<String, bool>,
) -> Vec<PlanAction> {
    let mut actions = Vec::new();
    let actual_by_path: HashMap<&str, &HarnessSkill> =
        actual.iter().map(|s| (s.path.as_str(), s)).collect();

    for d in desired {
        match actual_by_path.get(d.canonical_path.as_str()) {
            None => {
                let name_hit = actual.iter().find(|s| s.name == d.name);
                match name_hit {
                    Some(hit) if hit.symlinked => {
                        // already canonical via a symlink — nothing to do
                        actions.push(PlanAction::Unchanged(UnchangedAction {
                            kind: "skill".into(),
                            identity: d.name.clone(),
                        }));
                    }
                    Some(hit) if hit.content_hash != d.content_hash => {
                        actions.push(PlanAction::Conflict(ConflictAction {
                            kind: "skill".into(),
                            identity: d.name.clone(),
                            reason: "skill content changed outside CHM".into(),
                            desired: serde_json::json!(d),
                            current: serde_json::json!({
                                "name": hit.name,
                                "path": hit.path,
                                "content_hash": hit.content_hash,
                                "symlinked": hit.symlinked,
                            }),
                        }));
                    }
                    Some(_) => actions.push(PlanAction::Unchanged(UnchangedAction {
                        kind: "skill".into(),
                        identity: d.name.clone(),
                    })),
                    None => actions.push(PlanAction::Add(AddAction {
                        kind: "skill".into(),
                        identity: d.canonical_path.clone(),
                        payload: serde_json::json!(d),
                        native_provider_id: None,
                    })),
                }
            }
            Some(a) => {
                let same_name = a.name == d.name;
                if same_name {
                    actions.push(PlanAction::Unchanged(UnchangedAction {
                        kind: "skill".into(),
                        identity: d.name.clone(),
                    }));
                } else {
                    actions.push(PlanAction::Conflict(ConflictAction {
                        kind: "skill".into(),
                        identity: d.canonical_path.clone(),
                        reason: format!(
                            "path holds skill '{}' but canonical is '{}'",
                            a.name, d.name
                        ),
                        desired: serde_json::json!(d),
                        current: serde_json::json!({
                            "name": a.name,
                            "path": a.path,
                            "content_hash": a.content_hash,
                            "symlinked": a.symlinked,
                        }),
                    }));
                }
            }
        }
    }

    if matches!(mode, Mode::ReplaceManaged) {
        for a in actual {
            let key = format!("skill:{}", a.path);
            let is_managed = managed.get(&key).copied().unwrap_or(false);
            if !is_managed {
                continue;
            }
            let still_desired = desired
                .iter()
                .any(|d| d.canonical_path == a.path || d.name == a.name);
            if !still_desired {
                actions.push(PlanAction::Remove(RemoveAction {
                    kind: "skill".into(),
                    identity: a.path.clone(),
                    native_provider_id: None,
                }));
            }
        }
    }

    actions
}

/// Marks skill actions unsupported when the harness can't link skills.
pub fn reject_unsupported_skill_bindings(
    actions: Vec<PlanAction>,
    supports_links: bool,
) -> Vec<PlanAction> {
    if supports_links {
        return actions;
    }
    actions
        .into_iter()
        .map(|a| match &a {
            PlanAction::Add(x) if x.kind == "skill" => PlanAction::Unsupported(UnsupportedAction {
                kind: "skill".into(),
                identity: x.identity.clone(),
                reason: "harness does not support skill linking".into(),
            }),
            _ => a,
        })
        .collect()
}
