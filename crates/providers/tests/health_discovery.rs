use chm_core::domain::provider::*;
use chm_providers::{HealthStatus, discover_models, health_check};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn endpoint(base: &str, protocol: Protocol) -> ProviderEndpoint {
    ProviderEndpoint {
        id: uuid::Uuid::new_v4(),
        provider_id: uuid::Uuid::new_v4(),
        name: "mock".into(),
        base_url: base.into(),
        protocol,
        discovery_path: Some("/v1/models".into()),
        auth_type: AuthType::BearerToken,
        credential_ref: None,
        headers: Default::default(),
        enabled: true,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

#[tokio::test]
async fn health_check_reports_healthy_on_200() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"object": "list", "data": []})),
        )
        .mount(&server)
        .await;
    let http = reqwest::Client::new();
    let status = health_check(
        &endpoint(&server.uri(), Protocol::OpenAiChatCompletions),
        None,
        &http,
    )
    .await;
    assert_eq!(status, HealthStatus::Healthy);
}

#[tokio::test]
async fn health_check_detects_auth_failure() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let http = reqwest::Client::new();
    let status = health_check(
        &endpoint(&server.uri(), Protocol::OpenAiChatCompletions),
        None,
        &http,
    )
    .await;
    assert_eq!(status, HealthStatus::AuthFailed);
}

#[tokio::test]
async fn discovery_parses_openai_model_list() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "object": "list",
            "data": [{"id": "glm-5", "object": "model"}, {"id": "glm-5-air", "object": "model"}]
        })))
        .mount(&server)
        .await;
    let http = reqwest::Client::new();
    let models = discover_models(
        &endpoint(&server.uri(), Protocol::OpenAiChatCompletions),
        None,
        &http,
    )
    .await
    .unwrap();
    assert_eq!(models.len(), 2);
    assert_eq!(models[0].id, "glm-5");
}

#[tokio::test]
async fn unreachable_endpoint_reports_unreachable() {
    let http = reqwest::Client::new();
    let e = endpoint("http://127.0.0.1:1", Protocol::OpenAiChatCompletions);
    let status = health_check(&e, None, &http).await;
    assert_eq!(status, HealthStatus::Unreachable);
}
