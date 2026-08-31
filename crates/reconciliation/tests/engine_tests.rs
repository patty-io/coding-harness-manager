use chm_core::domain::mcp::{McpServer, McpTransport, ScopeType};
use chm_core::domain::models::ModelRoute;
use chm_core::domain::provider::Protocol;
use chm_core::domain::skills::Skill;
use chm_harness_sdk::adapter::route::{
    CredentialRequirement, ModelIdentityRules, ModelMetadataCapabilities, ProviderRouteBundle,
    ProviderTopology, RouteDeploymentCapabilities,
};
use chm_harness_sdk::adapter::types::HarnessCapabilities;
use chm_reconciliation::engine::{filter_unsupported, reconcile, reconcile_with_capabilities};
use chm_reconciliation::plan::{ActualState, DesiredState, Mode, PlanAction};
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

fn skill(name: &str) -> Skill {
    Skill {
        id: Uuid::new_v4(),
        name: name.into(),
        canonical_path: format!("/shared/skills/{name}"),
        source_type: chm_core::domain::skills::SkillSourceType::Folder,
        source_url: None,
        content_hash: None,
        provenance: serde_json::json!({}),
        enabled: true,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

#[test]
fn incompatible_bundle_is_a_route_blocker_not_a_model_action() {
    let endpoint = Uuid::new_v4();
    let model = route(endpoint, "qwen", None);
    let desired = DesiredState {
        provider_routes: vec![ProviderRouteBundle {
            provider_id: "yolo-auto".into(),
            display_name: "Yolo-Auto".into(),
            endpoint_id: endpoint,
            base_url: "https://yolo-auto.example/v1".into(),
            protocol: Protocol::OpenAiChatCompletions,
            credential: CredentialRequirement::None,
            models: vec![model.clone()],
        }],
        routes: vec![model],
        ..Default::default()
    };
    let capabilities = HarnessCapabilities::none()
        .with_models(true)
        .with_providers(true)
        .with_route_deployment(RouteDeploymentCapabilities {
            provider_topology: ProviderTopology::Multiple,
            protocols: vec![Protocol::OpenAiResponses],
            credential_targets: vec![],
            model_identity: ModelIdentityRules {
                case_sensitive: true,
                allow_namespaced_ids: true,
            },
            metadata: ModelMetadataCapabilities {
                context_window: true,
                max_input: true,
                max_output: true,
            },
        });

    let plan = reconcile_with_capabilities(
        &desired,
        &ActualState::default(),
        Mode::Append,
        &capabilities,
    )
    .unwrap();

    assert!(matches!(&plan.actions[0], PlanAction::Unsupported(action)
        if action.kind == "provider-route" && action.identity == "yolo-auto"));
    assert_eq!(plan.count("model"), 0);
}

#[test]
fn reconcile_dispatches_all_three_kinds() {
    let endpoint = Uuid::new_v4();
    let desired = DesiredState {
        provider_routes: vec![],
        routes: vec![route(endpoint, "glm-5", Some(1_048_576))],
        mcp_servers: vec![mcp_server("github")],
        skills: vec![skill("brainstorming")],
    };
    let actual = ActualState::default();
    let plan = reconcile(&desired, &actual, Mode::Append).unwrap();
    let kinds: Vec<&str> = plan
        .actions
        .iter()
        .map(|a| match a {
            PlanAction::Add(x) => x.kind.as_str(),
            _ => "other",
        })
        .collect();
    assert!(kinds.contains(&"model"));
    assert!(kinds.contains(&"mcp"));
    assert!(kinds.contains(&"skill"));
}

#[test]
fn reconcile_rejects_empty_remote_model_id() {
    let desired = DesiredState {
        routes: vec![route(Uuid::new_v4(), "", None)],
        ..Default::default()
    };
    let err = reconcile(&desired, &ActualState::default(), Mode::Append).unwrap_err();
    assert!(err.to_string().contains("empty"));
}

#[test]
fn filter_unsupported_converts_actions() {
    let endpoint = Uuid::new_v4();
    let desired = DesiredState {
        provider_routes: vec![],
        routes: vec![route(endpoint, "glm-5", Some(1_048_576))],
        mcp_servers: vec![mcp_server("github")],
        skills: vec![],
    };
    let plan = reconcile(&desired, &ActualState::default(), Mode::Append).unwrap();
    // harness supports MCP but NOT custom models
    let caps = HarnessCapabilities::none().with_mcp_global(true);
    let filtered = filter_unsupported(plan, &caps);
    let model_unsupported = filtered
        .actions
        .iter()
        .any(|a| matches!(a, PlanAction::Unsupported(u) if u.identity == "glm-5"));
    let mcp_added = filtered
        .actions
        .iter()
        .any(|a| matches!(a, PlanAction::Add(x) if x.kind == "mcp"));
    assert!(model_unsupported, "model action must become unsupported");
    assert!(mcp_added, "mcp action must survive");
}

#[test]
fn summary_renders_stable_dry_run_line() {
    let endpoint = Uuid::new_v4();
    let desired = DesiredState {
        provider_routes: vec![],
        routes: vec![route(endpoint, "glm-5", Some(1_048_576))],
        mcp_servers: vec![mcp_server("github")],
        skills: vec![skill("brainstorming")],
    };
    let plan = reconcile(&desired, &ActualState::default(), Mode::Append).unwrap();
    let s = plan.summary();
    assert!(s.contains("1 model"), "{s}");
    assert!(s.contains("1 MCP server"), "{s}");
    assert!(s.contains("1 skill"), "{s}");
    assert!(s.contains("will change"), "{s}");
}

#[test]
fn every_action_variant_reachable() {
    use chm_reconciliation::mcp_skills::{reconcile_mcp, reconcile_skills};
    use chm_reconciliation::models::reconcile_models;
    let endpoint = Uuid::new_v4();
    // Add + Update + Unchanged + Remove + Conflict + Unsupported + NoOp
    let desired = vec![
        route(endpoint, "add-me", None),
        route(endpoint, "update-me", Some(100)),
        route(endpoint, "same-me", None),
        route(endpoint, "alias-me", None),
    ];
    let mut upd = route(endpoint, "update-me", None);
    upd.context_window = Some(50);
    let mut alias = route(endpoint, "alias-me", None);
    alias.remote_model_id = "alias-me".into();
    let actual = vec![
        chm_harness_sdk::adapter::types::HarnessModel {
            native_id: "update-me".into(),
            route: upd,
        },
        chm_harness_sdk::adapter::types::HarnessModel {
            native_id: "native-other".into(),
            route: alias,
        },
        chm_harness_sdk::adapter::types::HarnessModel {
            native_id: "same-me".into(),
            route: route(endpoint, "same-me", None),
        },
    ];
    let plan = reconcile_models(
        &desired,
        &actual,
        Mode::ReplaceManaged,
        &managed_all(&actual),
    );
    let variants = variant_set(&plan);
    assert!(variants.contains("Add"));
    assert!(variants.contains("Update"));
    assert!(variants.contains("Unchanged"));
    assert!(variants.contains("Conflict"));
    let removes = reconcile_mcp(
        &[],
        &[harness_mcp("obsolete")],
        Mode::ReplaceManaged,
        &managed(&["mcp:obsolete"]),
    );
    assert!(variant_set(&removes).contains("Remove"));
    let unsup = filter_unsupported(
        chm_reconciliation::plan::ReconciliationPlan { actions: plan },
        &HarnessCapabilities::none(),
    );
    assert!(variant_set(&unsup.actions).contains("Unsupported"));
    let _ = reconcile_skills;
}

fn harness_mcp(name: &str) -> chm_harness_sdk::adapter::types::HarnessMcp {
    chm_harness_sdk::adapter::types::HarnessMcp {
        native_name: name.into(),
        server: McpServer {
            id: Uuid::new_v4(),
            name: name.into(),
            transport: McpTransport::Stdio,
            command: None,
            args: vec![],
            url: None,
            env: Default::default(),
            scope_type: ScopeType::Global,
            scope_path: None,
            provenance: serde_json::json!({}),
            enabled: true,
        },
    }
}

fn managed(ids: &[&str]) -> std::collections::HashMap<String, bool> {
    ids.iter().map(|s| (s.to_string(), true)).collect()
}

fn managed_all(
    actual: &[chm_harness_sdk::adapter::types::HarnessModel],
) -> std::collections::HashMap<String, bool> {
    actual
        .iter()
        .map(|m| {
            (
                format!("route:{}:{}", m.route.endpoint_id, m.native_id),
                true,
            )
        })
        .collect()
}

fn variant_set(actions: &[PlanAction]) -> std::collections::HashSet<String> {
    actions
        .iter()
        .map(|a| {
            match a {
                PlanAction::Add(_) => "Add",
                PlanAction::Update(_) => "Update",
                PlanAction::Remove(_) => "Remove",
                PlanAction::Unchanged(_) => "Unchanged",
                PlanAction::Conflict(_) => "Conflict",
                PlanAction::Unsupported(_) => "Unsupported",
                PlanAction::NoOp(_) => "NoOp",
            }
            .to_string()
        })
        .collect()
}
