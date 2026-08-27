use chm_core::domain::harness::HarnessType;
use chm_core::domain::mcp::*;
use chm_core::domain::profiles::*;
use chm_core::domain::sets::SetItemType;
use chm_core::domain::skills::*;
use chm_database::connect_test;
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
