use chm_core::domain::mcp::{McpServer, McpTransport, ScopeType};
use chm_core::domain::models::ModelRoute;
use chm_core::domain::skills::Skill;
use chm_harness_sdk::adapter::types::{HarnessMcp, HarnessModel, HarnessSkill};
use chm_reconciliation::models::reconcile_models;
use chm_reconciliation::plan::{Mode, PlanAction};
use chrono::Utc;
use uuid::Uuid;

fn route(endpoint_id: Uuid, remote_id: &str, context: Option<i64>) -> ModelRoute {
    let r = ModelRoute::new(
        remote_id.into(),
        remote_id.into(),
        context,
        serde_json::json!({}),
        serde_json::json!({}),
    );
    ModelRoute { endpoint_id, ..r }
}

fn actual_model(native_id: &str) -> HarnessModel {
    HarnessModel {
        native_id: native_id.into(),
        route: route(Uuid::new_v4(), native_id, None),
    }
}

fn managed(ids: &[&str]) -> std::collections::HashMap<String, bool> {
    ids.iter().map(|s| (s.to_string(), true)).collect()
}

#[test]
fn append_plans_add_for_missing_models() {
    let endpoint = Uuid::new_v4();
    let desired = vec![route(endpoint, "glm-5", Some(1_048_576))];
    let actual = vec![];
    let plan = reconcile_models(&desired, &actual, Mode::Append, &managed(&[]));
    assert_eq!(plan.len(), 1);
    assert!(matches!(&plan[0], PlanAction::Add(a) if a.identity == "glm-5"));
}

#[test]
fn append_never_plans_removes() {
    let desired: Vec<ModelRoute> = vec![];
    let actual = vec![actual_model("old-model")];
    let plan = reconcile_models(
        &desired,
        &actual,
        Mode::Append,
        &managed(&["route:x:old-model"]),
    );
    assert!(plan.is_empty(), "append mode must not remove");
}

#[test]
fn replace_managed_removes_only_managed_items() {
    let desired: Vec<ModelRoute> = vec![];
    let actual = vec![actual_model("managed-one"), actual_model("native-two")];
    let plan = reconcile_models(
        &desired,
        &actual,
        Mode::ReplaceManaged,
        &managed(&["route:x:managed-one"]),
    );
    let removes: Vec<_> = plan
        .iter()
        .filter_map(|a| match a {
            PlanAction::Remove(r) => Some(r.identity.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        removes,
        vec!["managed-one".to_string()],
        "only managed items removed"
    );
}

#[test]
fn update_when_only_metadata_changed() {
    let endpoint = Uuid::new_v4();
    let desired = vec![route(endpoint, "glm-5", Some(2_000_000))];
    let actual = vec![actual_model("glm-5")];
    let plan = reconcile_models(
        &desired,
        &actual,
        Mode::Append,
        &managed(&["route:x:glm-5"]),
    );
    assert_eq!(plan.len(), 1);
    match &plan[0] {
        PlanAction::Update(u) => {
            assert!(u.changed_fields.contains(&"context_window".to_string()));
            assert_eq!(u.identity, "glm-5");
        }
        other => panic!("expected Update, got {other:?}"),
    }
}

#[test]
fn unchanged_when_equal() {
    let endpoint = Uuid::new_v4();
    let desired = vec![route(endpoint, "glm-5", Some(1_048_576))];
    let mut actual = actual_model("glm-5");
    actual.route.context_window = Some(1_048_576);
    let plan = reconcile_models(
        &desired,
        &[actual],
        Mode::Append,
        &managed(&["route:x:glm-5"]),
    );
    assert!(matches!(&plan[0], PlanAction::Unchanged(_)));
}

#[test]
fn alias_collision_is_conflict() {
    let endpoint = Uuid::new_v4();
    let desired = vec![route(endpoint, "glm-5", None)];
    // harness exposes it under a different native id
    let mut actual = actual_model("glm-5-prod");
    actual.route.remote_model_id = "glm-5".into();
    let plan = reconcile_models(&desired, &[actual], Mode::Append, &managed(&[]));
    assert!(matches!(&plan[0], PlanAction::Conflict(c) if c.reason.contains("native id")));
}

fn mcp_server(name: &str) -> McpServer {
    McpServer {
        id: Uuid::new_v4(),
        name: name.into(),
        transport: McpTransport::Stdio,
        command: Some("npx".into()),
        args: vec![],
        url: None,
        env: Default::default(),
        scope_type: ScopeType::Global,
        scope_path: None,
        provenance: serde_json::json!({}),
        enabled: true,
    }
}

fn harness_mcp(name: &str) -> HarnessMcp {
    HarnessMcp {
        native_name: name.into(),
        server: mcp_server(name),
    }
}

#[test]
fn mcp_append_adds_and_preserves_existing() {
    use chm_reconciliation::mcp_skills::reconcile_mcp;
    let desired = vec![mcp_server("github"), mcp_server("playwright")];
    let actual = vec![harness_mcp("github")];
    let plan = reconcile_mcp(&desired, &actual, Mode::Append, &managed(&[]));
    let adds: Vec<_> = plan
        .iter()
        .filter_map(|a| match a {
            PlanAction::Add(x) => Some(x.identity.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(adds, vec!["playwright"]);
}

#[test]
fn mcp_removal_only_in_replace_managed_for_managed_items() {
    use chm_reconciliation::mcp_skills::reconcile_mcp;
    let desired: Vec<McpServer> = vec![];
    let actual = vec![harness_mcp("obsolete")];
    let plan = reconcile_mcp(
        &desired,
        &actual,
        Mode::ReplaceManaged,
        &managed(&["mcp:obsolete"]),
    );
    assert!(matches!(&plan[0], PlanAction::Remove(r) if r.identity == "obsolete"));
}

#[test]
fn skills_add_unchanged_conflict_unsupported() {
    use chm_reconciliation::mcp_skills::{reconcile_skills, reject_unsupported_skill_bindings};
    let skill = |name: &str, path: &str| Skill {
        id: Uuid::new_v4(),
        name: name.into(),
        canonical_path: path.into(),
        source_type: chm_core::domain::skills::SkillSourceType::Folder,
        source_url: None,
        content_hash: Some("abc".into()),
        provenance: serde_json::json!({}),
        enabled: true,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    let desired = vec![
        skill("brainstorming", "/shared/skills/brainstorming"),
        skill("existing", "/shared/skills/existing"),
    ];
    let actual = vec![
        HarnessSkill {
            name: "brainstorming".into(),
            path: "/harness/skills/brainstorming".into(),
            content_hash: Some("different".into()),
            symlinked: false,
        },
        HarnessSkill {
            name: "existing".into(),
            path: "/shared/skills/existing".into(),
            content_hash: Some("abc".into()),
            symlinked: true,
        },
    ];
    let plan = reconcile_skills(&desired, &actual, Mode::Append, &managed(&[]));
    // brainstorming: name conflict (different content) -> Conflict
    // existing: symlinked already -> Unchanged
    let conflicts = plan
        .iter()
        .filter(|a| matches!(a, PlanAction::Conflict(_)))
        .count();
    assert_eq!(conflicts, 1, "content mismatch must be a conflict");
    let unchanged = plan
        .iter()
        .filter(|a| matches!(a, PlanAction::Unchanged(_)))
        .count();
    assert_eq!(unchanged, 1);

    // unsupported binding: adds become Unsupported
    let rejected = reject_unsupported_skill_bindings(plan, false);
    assert!(
        rejected
            .iter()
            .any(|a| matches!(a, PlanAction::Unsupported(_)))
    );
    assert!(
        rejected
            .iter()
            .any(|a| matches!(a, PlanAction::Conflict(_))),
        "conflicts survive"
    );
}
