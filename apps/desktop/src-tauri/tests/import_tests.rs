use chm_core::domain::harness::{HarnessInstallation, HarnessType, InstallationStatus};
use chm_core::domain::mcp::{McpServer, McpTransport, ScopeType};
use chm_database::connect_test;
use chm_database::repos::harness::upsert_installation;
use chm_database::repos::mcp::list_mcp_servers;
use chm_database::repos::providers::{create_provider, list_providers};
use chm_database::repos::skills::list_skills;
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
async fn failed_import_rolls_back_entire_transaction() {
    let pool = connect_test().await.unwrap();
    // the SAME mcp name arrives twice in the parsed state: once from the
    // opencode.jsonc `mcp` object and once from opencode-mcp.json. The second
    // insert violates UNIQUE(name) mid-batch; the whole import must roll back.
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
    let err = run_import(&pool, &inst, &parsed.2, true, true, true)
        .await
        .expect_err("duplicate mcp name in one batch must fail the import");
    assert!(
        err.contains("import failed") || err.contains("UNIQUE"),
        "got: {err}"
    );
    // nothing persisted: provider + mcp + routes all rolled back
    assert!(
        list_providers(&pool).await.unwrap().is_empty(),
        "providers must roll back"
    );
    assert!(
        list_mcp_servers(&pool).await.unwrap().is_empty(),
        "mcp must roll back"
    );
}

#[tokio::test]
async fn reimport_links_routes_to_existing_provider_endpoint() {
    let pool = connect_test().await.unwrap();
    let config = r#"{
      "provider": {
        "zai": {
          "npm": "@ai-sdk/anthropic",
          "options": { "baseURL": "https://api.z.ai/api/anthropic", "apiKey": "{env:ZAI_API_KEY}" },
          "models": { "glm-5": { "name": "GLM-5", "limit": { "context": 1048576 } } }
        }
      }
    }"#;
    let (_dir, inst) = fixture_install(config);
    upsert_installation(&pool, &inst).await.unwrap();

    let parsed = coding_harness_manager_lib::commands::import::read_parsed_state(
        &pool,
        &inst.id.to_string(),
    )
    .await
    .unwrap();
    let first = run_import(&pool, &inst, &parsed.2, true, true, true)
        .await
        .unwrap();
    assert_eq!(first.models_imported, 1);
    // second import: provider exists -> not recreated; route links to the
    // EXISTING endpoint, so the unique (endpoint_id, remote_model_id) hits
    // and the model is reported as a duplicate — no junk "imported" provider.
    let parsed2 = coding_harness_manager_lib::commands::import::read_parsed_state(
        &pool,
        &inst.id.to_string(),
    )
    .await
    .unwrap();
    let second = run_import(&pool, &inst, &parsed2.2, true, true, true)
        .await
        .unwrap();
    assert_eq!(second.providers_created, 0);
    assert_eq!(second.models_imported, 0);
    assert!(
        second.duplicates.iter().any(|d| d.starts_with("model:")),
        "got: {:?}",
        second.duplicates
    );
    assert!(
        second.duplicates.iter().any(|d| d.starts_with("provider:")),
        "existing provider reported as duplicate"
    );
    let providers = list_providers(&pool).await.unwrap();
    assert_eq!(
        providers.len(),
        1,
        "no placeholder 'imported' provider may be created"
    );
    assert_eq!(providers[0].name, "zai");
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
