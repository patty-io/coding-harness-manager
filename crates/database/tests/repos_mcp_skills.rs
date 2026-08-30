use chm_core::domain::harness::{
    HarnessInstallation, HarnessMcpBinding, HarnessType, InstallationStatus,
};
use chm_core::domain::mcp::*;
use chm_core::domain::profiles::*;
use chm_core::domain::sets::SetItemType;
use chm_core::domain::skills::*;
use chm_database::connect_test;
use chm_database::repos::harness::upsert_installation;
use chm_database::repos::mcp::*;
use chm_database::repos::profiles::*;
use chm_database::repos::skills::*;

#[tokio::test]
async fn mcp_crud_flow() {
    let pool = connect_test().await.unwrap();
    let s = McpServer {
        id: uuid::Uuid::new_v4(),
        name: "github".into(),
        transport: McpTransport::Stdio,
        command: Some("npx".into()),
        args: vec!["-y".into(), "@modelcontextprotocol/server-github".into()],
        url: None,
        env: serde_json::json!({"GITHUB_PERSONAL_ACCESS_TOKEN": "$LP_GITHUB_TOKEN"})
            .as_object()
            .unwrap()
            .clone(),
        scope_type: ScopeType::Global,
        scope_path: None,
        provenance: serde_json::json!({"source": "manual"}),
        enabled: true,
    };
    create_mcp_server(&pool, &s).await.unwrap();
    let all = list_mcp_servers(&pool).await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].transport, McpTransport::Stdio);
    assert_eq!(all[0].env.len(), 1);
    delete_mcp_server(&pool, s.id).await.unwrap();
    assert!(list_mcp_servers(&pool).await.unwrap().is_empty());
}

#[tokio::test]
async fn mcp_bindings_can_be_listed_by_server_without_installation_n_plus_one() {
    let pool = connect_test().await.unwrap();
    let server = McpServer {
        id: uuid::Uuid::new_v4(),
        name: "filesystem".into(),
        transport: McpTransport::Stdio,
        command: Some("mcp-filesystem".into()),
        args: vec![],
        url: None,
        env: Default::default(),
        scope_type: ScopeType::Global,
        scope_path: None,
        provenance: Default::default(),
        enabled: true,
    };
    create_mcp_server(&pool, &server).await.unwrap();
    let installation = HarnessInstallation {
        id: uuid::Uuid::new_v4(),
        harness_type: HarnessType::Pi,
        executable_path: None,
        version: Some("0.84.3".into()),
        config_path: None,
        detected_at: chrono::Utc::now(),
        last_scanned_at: None,
        status: InstallationStatus::Installed,
    };
    let stored = upsert_installation(&pool, &installation).await.unwrap();
    let binding = HarnessMcpBinding {
        id: uuid::Uuid::new_v4(),
        harness_installation_id: stored.id,
        mcp_server_id: server.id,
        native_name: server.name.clone(),
        native_config: Default::default(),
        managed: true,
    };
    create_mcp_binding(&pool, &binding).await.unwrap();

    let rows = list_mcp_bindings_for_server(&pool, server.id)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0.id, binding.id);
    assert_eq!(rows[0].1, HarnessType::Pi);
}

#[tokio::test]
async fn skill_and_profile_and_set_flow() {
    let pool = connect_test().await.unwrap();
    let sk = Skill {
        id: uuid::Uuid::new_v4(),
        name: "brainstorming".into(),
        canonical_path: "/Users/me/.agents/skills/brainstorming".into(),
        source_type: SkillSourceType::Folder,
        source_url: None,
        content_hash: Some("abc123".into()),
        provenance: serde_json::json!({"source": "imported"}),
        enabled: true,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    create_skill(&pool, &sk).await.unwrap();
    assert_eq!(list_skills(&pool).await.unwrap().len(), 1);

    let p = LaunchProfile {
        id: uuid::Uuid::new_v4(),
        name: "zai-claude".into(),
        harness_type: HarnessType::ClaudeCode,
        model_route_id: None,
        provider_endpoint_id: None,
        env: serde_json::json!({"ANTHROPIC_BASE_URL": "https://api.z.ai/api/anthropic"})
            .as_object()
            .unwrap()
            .clone(),
        role_mappings: vec![RoleMapping {
            role: "opus".into(),
            model: "glm-5".into(),
        }],
        native_overrides: serde_json::Value::Object(Default::default()),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    create_profile(&pool, &p).await.unwrap();
    assert_eq!(list_profiles(&pool).await.unwrap().len(), 1);
    assert_eq!(
        list_profiles(&pool).await.unwrap()[0].role_mappings[0].model,
        "glm-5"
    );

    let set = create_set(&pool, "Work", Some("work models".into()))
        .await
        .unwrap();
    add_set_item(&pool, set.id, SetItemType::ModelRoute, uuid::Uuid::new_v4())
        .await
        .unwrap();
    assert_eq!(list_sets(&pool).await.unwrap().len(), 1);
}
