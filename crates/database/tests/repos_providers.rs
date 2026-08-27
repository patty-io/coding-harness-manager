use chm_core::domain::credentials::*;
use chm_core::domain::provider::*;
use chm_database::connect_test;
use chm_database::repos::providers::*;

#[tokio::test]
async fn provider_crud_flow() {
    let pool = connect_test().await.unwrap();
    let p = create_provider(&pool, "zai", "Z.AI").await.unwrap();
    assert_eq!(p.name, "zai");

    let list = list_providers(&pool).await.unwrap();
    assert_eq!(list.len(), 1);

    let updated = update_provider(
        &pool,
        p.id,
        "Z.AI (kr)",
        true,
        Some("korean provider".into()),
    )
    .await
    .unwrap();
    assert_eq!(updated.display_name, "Z.AI (kr)");

    delete_provider(&pool, p.id).await.unwrap();
    assert!(list_providers(&pool).await.unwrap().is_empty());
}

#[tokio::test]
async fn endpoint_and_credential_flow() {
    let pool = connect_test().await.unwrap();
    let p = create_provider(&pool, "minimax", "MiniMax").await.unwrap();
    let cred = create_credential_ref(&pool, CredentialKind::Env, "MINIMAX_API_KEY")
        .await
        .unwrap();
    let e = ProviderEndpoint {
        id: uuid::Uuid::new_v4(),
        provider_id: p.id,
        name: "openai-compat".into(),
        base_url: "https://api.minimaxi.com/v1".into(),
        protocol: Protocol::OpenAiChatCompletions,
        discovery_path: Some("/models".into()),
        auth_type: AuthType::BearerToken,
        credential_ref: Some(cred),
        headers: Default::default(),
        enabled: true,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    create_endpoint(&pool, &e).await.unwrap();
    let endpoints = list_endpoints(&pool, p.id).await.unwrap();
    assert_eq!(endpoints.len(), 1);
    assert_eq!(endpoints[0].protocol, Protocol::OpenAiChatCompletions);
    assert_eq!(
        endpoints[0].credential_ref.as_ref().unwrap().reference,
        "MINIMAX_API_KEY"
    );
}
