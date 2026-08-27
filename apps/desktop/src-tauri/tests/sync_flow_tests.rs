use chm_core::domain::harness::{HarnessInstallation, HarnessType, InstallationStatus};
use chm_database::connect_test;
use chm_database::repos::history::{list_snapshots, list_transactions};
use chm_database::repos::harness::upsert_installation;
use chm_database::repos::models::create_route;
use chm_database::repos::providers::{create_endpoint, create_provider};
use chm_core::domain::history::TransactionStatus;
use chrono::Utc;
use coding_harness_manager_lib::commands::sync::execute_sync;
use chm_harness_sdk::adapter::plan::Mode;
use uuid::Uuid;

#[tokio::test]
async fn execute_sync_applies_and_records_snapshots() {
    let pool = connect_test().await.unwrap();
    let dir = tempfile::TempDir::new().unwrap();
    // seed: provider + endpoint + one route (desired)
    let p = create_provider(&pool, "zai", "Z.AI").await.unwrap();
    let e = chm_core::domain::provider::ProviderEndpoint {
        id: Uuid::new_v4(),
        provider_id: p.id,
        name: "anthropic".into(),
        base_url: "https://api.z.ai/api/anthropic".into(),
        protocol: chm_core::domain::provider::Protocol::AnthropicMessages,
        discovery_path: Some("/v1/models".into()),
        auth_type: chm_core::domain::provider::AuthType::BearerToken,
        credential_ref: None,
        headers: Default::default(),
        enabled: true,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    create_endpoint(&pool, &e).await.unwrap();
    let route = chm_core::domain::models::ModelRoute::new(
        "glm-5".into(),
        "GLM-5".into(),
        Some(1_048_576),
        serde_json::json!({}),
        serde_json::json!({"native_provider_id": "zai"}),
    );
    let route = chm_core::domain::models::ModelRoute {
        endpoint_id: e.id,
        ..route
    };
    create_route(&pool, &route).await.unwrap();

    // seed: installation pointing at a fresh temp config file
    let inst = HarnessInstallation {
        id: Uuid::new_v4(),
        harness_type: HarnessType::OpenCode,
        executable_path: None,
        version: Some("0.30.0".into()),
        config_path: Some(dir.path().join("opencode.jsonc").display().to_string()),
        detected_at: Utc::now(),
        last_scanned_at: Some(Utc::now()),
        status: InstallationStatus::Installed,
    };
    upsert_installation(&pool, &inst).await.unwrap();
    chm_filesystem::atomic_write(&dir.path().join("opencode.jsonc"), "{}").unwrap();

    let report = execute_sync(&pool, &inst.id.to_string(), &Mode::Append, false)
        .await
        .unwrap();
    assert_eq!(report.files_written.len(), 1);
    // file now contains the model under the zai provider subtree
    let content = std::fs::read_to_string(dir.path().join("opencode.jsonc")).unwrap();
    assert!(content.contains("glm-5"));
    // transaction + snapshot recorded
    let txs = list_transactions(&pool).await.unwrap();
    assert_eq!(txs.len(), 1);
    assert_eq!(txs[0].status, TransactionStatus::Succeeded);
    let snaps = list_snapshots(&pool, txs[0].id).await.unwrap();
    assert_eq!(snaps.len(), 1);
    assert_eq!(snaps[0].before_content.as_deref(), Some("{}"));
    assert!(snaps[0].after_content.as_deref().unwrap_or("").contains("glm-5"));
}

#[tokio::test]
async fn sync_preview_is_idempotent_after_apply() {
    let pool = connect_test().await.unwrap();
    let dir = tempfile::TempDir::new().unwrap();
    let p = create_provider(&pool, "zai", "Z.AI").await.unwrap();
    let e = chm_core::domain::provider::ProviderEndpoint {
        id: Uuid::new_v4(),
        provider_id: p.id,
        name: "anthropic".into(),
        base_url: "https://api.z.ai/api/anthropic".into(),
        protocol: chm_core::domain::provider::Protocol::AnthropicMessages,
        discovery_path: Some("/v1/models".into()),
        auth_type: chm_core::domain::provider::AuthType::BearerToken,
        credential_ref: None,
        headers: Default::default(),
        enabled: true,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    create_endpoint(&pool, &e).await.unwrap();
    let route = chm_core::domain::models::ModelRoute::new(
        "glm-5".into(),
        "GLM-5".into(),
        Some(1_048_576),
        serde_json::json!({}),
        serde_json::json!({"native_provider_id": "zai"}),
    );
    let route = chm_core::domain::models::ModelRoute {
        endpoint_id: e.id,
        ..route
    };
    create_route(&pool, &route).await.unwrap();
    let inst = HarnessInstallation {
        id: Uuid::new_v4(),
        harness_type: HarnessType::OpenCode,
        executable_path: None,
        version: Some("0.30.0".into()),
        config_path: Some(dir.path().join("opencode.jsonc").display().to_string()),
        detected_at: Utc::now(),
        last_scanned_at: None,
        status: InstallationStatus::Installed,
    };
    upsert_installation(&pool, &inst).await.unwrap();
    chm_filesystem::atomic_write(&dir.path().join("opencode.jsonc"), "{}").unwrap();

    execute_sync(&pool, &inst.id.to_string(), &Mode::Append, false)
        .await
        .unwrap();
    // second run must plan zero changes
    let (_, _, plan, native_plan) =
        coding_harness_manager_lib::commands::sync::build_native_plan(
            &pool,
            &inst.id.to_string(),
            &Mode::Append,
        )
        .await
        .unwrap();
    let mutating = plan
        .actions
        .iter()
        .filter(|a| {
            matches!(
                a,
                chm_harness_sdk::adapter::plan::PlanAction::Add(_)
                    | chm_harness_sdk::adapter::plan::PlanAction::Update(_)
                    | chm_harness_sdk::adapter::plan::PlanAction::Remove(_)
            )
        })
        .count();
    assert_eq!(mutating, 0, "second sync must be a no-op: {:?}", plan.actions);
    assert!(native_plan.changes.is_empty(), "second sync must be a no-op");
}