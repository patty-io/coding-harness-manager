use chm_core::domain::harness::{HarnessInstallation, HarnessType, InstallationStatus};
use chm_core::domain::mcp::{McpServer, McpTransport, ScopeType};
use chm_core::domain::models::ModelRoute;
use chm_core::domain::provider::{AuthType, Protocol, ProviderEndpoint};
use chm_database::connect_test;
use chm_database::repos::harness::upsert_installation;
use chm_database::repos::mcp::list_mcp_servers;
use chm_database::repos::models::list_routes;
use chm_database::repos::providers::create_endpoint;
use chm_database::repos::providers::{create_provider, list_providers};
use chm_database::repos::skills::list_skills;
use chm_harness_sdk::adapter::types::{HarnessModel, ParsedState};
use chrono::Utc;
use coding_harness_manager_lib::services::import::run_import;
use uuid::Uuid;

/// Builds a temp "machine" with an opencode.jsonc config; returns install + config path.
fn fixture_install(config_json: &str) -> (tempfile::TempDir, HarnessInstallation) {
    let dir = tempfile::TempDir::new().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(home.join(".config/opencode")).unwrap();
    std::fs::write(home.join(".config/opencode/opencode.jsonc"), config_json).unwrap();
    let inst = HarnessInstallation {
        id: Uuid::new_v4(),
        harness_type: HarnessType::OpenCode,
        executable_path: Some("/fake/opencode".into()),
        version: Some("0.30.0".into()),
        config_path: Some(
            home.join(".config/opencode/opencode.jsonc")
                .display()
                .to_string(),
        ),
        detected_at: Utc::now(),
        last_scanned_at: None,
        status: InstallationStatus::Installed,
    };
    (dir, inst)
}

#[tokio::test]
async fn import_creates_provider_model_and_mcp() {
    let pool = connect_test().await.unwrap();
    let (_dir, inst) = fixture_install(
        r#"{
          "provider": {
            "zai": {
              "npm": "@ai-sdk/anthropic",
              "options": { "baseURL": "https://api.z.ai/api/anthropic", "apiKey": "{env:ZAI_API_KEY}" },
              "models": { "glm-5": { "name": "GLM-5", "limit": { "context": 1048576 } } }
            }
          },
          "mcp": {
            "github": { "type": "local", "command": ["npx", "-y", "@modelcontextprotocol/server-github"], "enabled": true }
          }
        }"#,
    );
    upsert_installation(&pool, &inst).await.unwrap();

    let parsed = coding_harness_manager_lib::commands::import::read_parsed_state(
        &pool,
        &inst.id.to_string(),
    )
    .await
    .unwrap();
    let report = run_import(&pool, &inst, &parsed.2, true, true, true)
        .await
        .unwrap();

    assert_eq!(report.providers_created, 1);
    assert_eq!(report.models_imported, 1);
    assert_eq!(report.mcp_imported, 1);
    assert!(report.duplicates.is_empty());
    assert_eq!(list_providers(&pool).await.unwrap().len(), 1);
    assert_eq!(list_mcp_servers(&pool).await.unwrap().len(), 1);
    assert_eq!(list_skills(&pool).await.unwrap().len(), 0);

    // provider got an env credential reference, never an inline value
    let providers = list_providers(&pool).await.unwrap();
    let endpoints = chm_database::repos::providers::list_endpoints(&pool, providers[0].id)
        .await
        .unwrap();
    assert_eq!(endpoints.len(), 1);
    assert_eq!(
        endpoints[0].credential_ref.as_ref().unwrap().reference,
        "ZAI_API_KEY"
    );
    assert_eq!(
        endpoints[0].credential_ref.as_ref().unwrap().kind,
        chm_core::domain::credentials::CredentialKind::Env
    );
}

#[tokio::test]
async fn in_batch_duplicate_mcp_is_reported_not_fatal() {
    let pool = connect_test().await.unwrap();
    // the SAME mcp name arrives twice: once from opencode.jsonc `mcp` and once
    // from opencode-mcp.json. In-batch duplicates are reported, not fatal.
    let dir = tempfile::TempDir::new().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(home.join(".config/opencode")).unwrap();
    std::fs::write(
        home.join(".config/opencode/opencode.jsonc"),
        r#"{
          "provider": {
            "zai": { "npm": "@ai-sdk/anthropic", "models": { "glm-5": { "name": "GLM-5" } } }
          },
          "mcp": {
            "github": { "type": "local", "command": ["npx", "-y", "server-github"], "enabled": true }
          }
        }"#,
    )
    .unwrap();
    std::fs::write(
        home.join(".config/opencode/opencode-mcp.json"),
        r#"{
          "github": { "type": "local", "command": ["npx", "-y", "server-github-2"], "enabled": true }
        }"#,
    )
    .unwrap();
    let inst = HarnessInstallation {
        id: Uuid::new_v4(),
        harness_type: HarnessType::OpenCode,
        executable_path: Some("/fake/opencode".into()),
        version: Some("0.30.0".into()),
        config_path: Some(
            home.join(".config/opencode/opencode.jsonc")
                .display()
                .to_string(),
        ),
        detected_at: Utc::now(),
        last_scanned_at: None,
        status: InstallationStatus::Installed,
    };
    upsert_installation(&pool, &inst).await.unwrap();

    let parsed = coding_harness_manager_lib::commands::import::read_parsed_state(
        &pool,
        &inst.id.to_string(),
    )
    .await
    .unwrap();
    let report = run_import(&pool, &inst, &parsed.2, true, true, true)
        .await
        .unwrap();

    assert_eq!(report.mcp_imported, 1, "first occurrence imports");
    assert!(
        report.duplicates.iter().any(|d| d.starts_with("mcp:")),
        "second occurrence reported as duplicate"
    );
    assert_eq!(list_mcp_servers(&pool).await.unwrap().len(), 1);
}

#[tokio::test]
async fn import_reports_duplicates_without_overwriting() {
    let pool = connect_test().await.unwrap();
    let (_dir, inst) = fixture_install(
        r#"{
          "provider": {
            "zai": { "npm": "@ai-sdk/anthropic", "models": { "glm-5": { "name": "GLM-5" } } }
          },
          "mcp": {
            "github": { "type": "local", "command": ["npx", "-y", "server-github"], "enabled": true }
          }
        }"#,
    );
    upsert_installation(&pool, &inst).await.unwrap();

    create_provider(&pool, "zai", "Z.AI").await.unwrap();
    let existing = McpServer {
        id: Uuid::new_v4(),
        name: "github".into(),
        transport: McpTransport::Stdio,
        command: Some("node".into()),
        args: vec![],
        url: None,
        env: Default::default(),
        scope_type: ScopeType::Global,
        scope_path: None,
        provenance: serde_json::json!({}),
        enabled: true,
    };
    chm_database::repos::mcp::create_mcp_server(&pool, &existing)
        .await
        .unwrap();

    let parsed = coding_harness_manager_lib::commands::import::read_parsed_state(
        &pool,
        &inst.id.to_string(),
    )
    .await
    .unwrap();
    let report = run_import(&pool, &inst, &parsed.2, true, true, true)
        .await
        .unwrap();

    assert_eq!(
        report.providers_created, 0,
        "provider name conflict must not overwrite"
    );
    assert_eq!(
        report.mcp_imported, 0,
        "mcp name conflict must not overwrite"
    );
    assert_eq!(report.duplicates.len(), 2);
    let servers = list_mcp_servers(&pool).await.unwrap();
    assert_eq!(servers.len(), 1);
    assert_eq!(
        servers[0].command.as_deref(),
        Some("node"),
        "existing server untouched"
    );
}

#[tokio::test]
async fn import_matches_existing_provider_endpoint_by_declared_base_url() {
    let pool = connect_test().await.unwrap();
    let provider = create_provider(&pool, "gateway", "Gateway").await.unwrap();
    let first = ProviderEndpoint {
        id: Uuid::new_v4(),
        provider_id: provider.id,
        name: "primary".into(),
        base_url: "https://primary.example/v1".into(),
        protocol: Protocol::OpenAiChatCompletions,
        discovery_path: Some("/v1/models".into()),
        auth_type: AuthType::None,
        credential_ref: None,
        headers: Default::default(),
        enabled: true,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    let declared = ProviderEndpoint {
        id: Uuid::new_v4(),
        provider_id: provider.id,
        name: "declared".into(),
        base_url: "https://declared.example/v1".into(),
        protocol: Protocol::OpenAiChatCompletions,
        discovery_path: Some("/v1/models".into()),
        auth_type: AuthType::None,
        credential_ref: None,
        headers: Default::default(),
        enabled: true,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    create_endpoint(&pool, &first).await.unwrap();
    create_endpoint(&pool, &declared).await.unwrap();

    let parsed = ParsedState {
        providers: vec![serde_json::json!({
            "native_provider_id": "gateway",
            "base_url": "https://declared.example/v1",
            "api": "openai-chat"
        })],
        models: vec![HarnessModel {
            native_id: "model-a".into(),
            route: ModelRoute::new(
                "model-a".into(),
                "Model A".into(),
                None,
                serde_json::json!({}),
                serde_json::json!({"native_provider_id": "gateway"}),
            ),
        }],
        ..Default::default()
    };
    let inst = HarnessInstallation {
        id: Uuid::new_v4(),
        harness_type: HarnessType::OpenCode,
        executable_path: None,
        version: None,
        config_path: None,
        detected_at: Utc::now(),
        last_scanned_at: None,
        status: InstallationStatus::Installed,
    };

    let report = run_import(&pool, &inst, &parsed, true, false, false)
        .await
        .unwrap();
    assert_eq!(report.models_imported, 1);
    let routes = list_routes(&pool).await.unwrap();
    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0].endpoint_id, declared.id);
}

#[tokio::test]
async fn importing_duplicate_skills_in_one_batch_is_non_fatal() {
    let pool = connect_test().await.unwrap();
    let dir = tempfile::TempDir::new().unwrap();
    let first = dir.path().join("first");
    let second = dir.path().join("second");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&second).unwrap();
    std::fs::write(first.join("SKILL.md"), "same content").unwrap();
    std::fs::write(second.join("SKILL.md"), "same content").unwrap();

    let first = first.display().to_string();
    let second = second.display().to_string();
    let report = coding_harness_manager_lib::commands::skills::import_skills_core(
        &pool,
        &[first.clone(), first, second],
    )
    .await
    .unwrap();

    assert_eq!(report.imported, 1);
    assert_eq!(report.duplicates.len(), 2);
    assert!(report.conflicts.is_empty());
    assert_eq!(list_skills(&pool).await.unwrap().len(), 1);
}
