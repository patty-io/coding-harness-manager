use chm_core::domain::models::*;
use chm_core::domain::provider::*;
use chm_database::connect_test;
use chm_database::repos::models::*;
use chm_database::repos::providers::*;

async fn endpoint_for(
    pool: &sqlx::Pool<sqlx::Sqlite>,
    provider_id: uuid::Uuid,
) -> ProviderEndpoint {
    let e = ProviderEndpoint {
        id: uuid::Uuid::new_v4(),
        provider_id,
        name: "anthropic".into(),
        base_url: "https://api.z.ai/api/anthropic".into(),
        protocol: Protocol::AnthropicMessages,
        discovery_path: Some("/v1/models".into()),
        auth_type: AuthType::BearerToken,
        credential_ref: None,
        headers: Default::default(),
        enabled: true,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    create_endpoint(pool, &e).await.unwrap();
    e
}

#[tokio::test]
async fn catalog_upsert_deduplicates_by_endpoint_and_remote_id() {
    let pool = connect_test().await.unwrap();
    let p = create_provider(&pool, "zai", "Z.AI").await.unwrap();
    let e = endpoint_for(&pool, p.id).await;
    let now = chrono::Utc::now();

    let m1 = ProviderCatalogModel {
        id: uuid::Uuid::new_v4(),
        endpoint_id: e.id,
        remote_model_id: "glm-5".into(),
        raw_metadata: serde_json::json!({"id": "glm-5"}),
        canonical_model_id: None,
        match_confidence: None,
        first_seen_at: now,
        last_seen_at: now,
        missing_since: None,
        status: CatalogStatus::New,
    };
    upsert_catalog_model(&pool, &m1).await.unwrap();
    let m2 = ProviderCatalogModel {
        id: uuid::Uuid::new_v4(),
        last_seen_at: now,
        ..m1.clone()
    };
    upsert_catalog_model(&pool, &m2).await.unwrap();

    let all = list_catalog_models(&pool, e.id).await.unwrap();
    assert_eq!(all.len(), 1, "upsert must not duplicate");
    assert_eq!(all[0].status, CatalogStatus::New);
}

#[tokio::test]
async fn route_crud_flow() {
    let pool = connect_test().await.unwrap();
    let p = create_provider(&pool, "openrouter", "OpenRouter")
        .await
        .unwrap();
    let e = ProviderEndpoint {
        id: uuid::Uuid::new_v4(),
        provider_id: p.id,
        name: "openai".into(),
        base_url: "https://openrouter.ai/api/v1".into(),
        protocol: Protocol::OpenRouterOpenAi,
        discovery_path: Some("/models".into()),
        auth_type: AuthType::BearerToken,
        credential_ref: None,
        headers: Default::default(),
        enabled: true,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    create_endpoint(&pool, &e).await.unwrap();
    let r = ModelRoute {
        id: uuid::Uuid::new_v4(),
        endpoint_id: e.id,
        model_identity_id: None,
        remote_model_id: "anthropic/claude-opus".into(),
        display_name: "Claude Opus via OpenRouter".into(),
        context_window: Some(200_000),
        max_input: None,
        max_output: None,
        capabilities: serde_json::json!({"reasoning": true}),
        overrides: serde_json::Value::Object(Default::default()),
        enabled: true,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    create_route(&pool, &r).await.unwrap();
    let dup = ModelRoute {
        id: uuid::Uuid::new_v4(),
        ..r.clone()
    };
    assert!(
        create_route(&pool, &dup).await.is_err(),
        "unique (endpoint_id, remote_model_id) must reject dup"
    );
    assert_eq!(list_routes(&pool).await.unwrap().len(), 1);
    delete_route(&pool, r.id).await.unwrap();
    assert!(list_routes(&pool).await.unwrap().is_empty());
}
