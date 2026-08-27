//! Provider HTTP client: health checks and /v1/models discovery.

use chm_core::domain::credentials::CredentialRef;
use chm_core::domain::provider::{AuthType, Protocol, ProviderEndpoint};
use chm_secrets::SecretStore;
use reqwest::StatusCode;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("authentication failed")]
    Auth,
    #[error("rate limited")]
    RateLimit,
    #[error("malformed response")]
    Malformed,
    #[error("endpoint unreachable")]
    Unreachable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthStatus {
    Healthy,
    AuthFailed,
    Unreachable,
    DiscoveryUnsupported,
    RateLimited,
    MalformedResponse,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct ProviderModel {
    pub id: String,
    pub raw: serde_json::Value,
}

pub fn resolve_credential(ref_: &CredentialRef, store: &dyn SecretStore) -> Option<String> {
    match ref_.kind {
        chm_core::domain::credentials::CredentialKind::Env => std::env::var(&ref_.reference).ok(),
        _ => store.get(&ref_.reference).ok().flatten(),
    }
}

fn discovery_url(endpoint: &ProviderEndpoint) -> String {
    let path = endpoint.discovery_path.as_deref().unwrap_or("/v1/models");
    format!("{}{}", endpoint.base_url.trim_end_matches('/'), path)
}

fn request_builder(
    http: &reqwest::Client,
    endpoint: &ProviderEndpoint,
    credential: Option<&str>,
    url: &str,
) -> reqwest::RequestBuilder {
    let mut req = http.get(url);
    match endpoint.auth_type {
        AuthType::BearerToken => {
            if let Some(c) = credential {
                req = req.bearer_auth(c);
            }
        }
        AuthType::ApiKeyHeader => {
            if let Some(c) = credential {
                req = req.header("x-api-key", c);
            }
        }
        AuthType::CustomHeader => {
            if let Some(c) = credential {
                req = req.header("authorization", c);
            }
        }
        AuthType::None | AuthType::Unknown => {}
    }
    for (k, v) in &endpoint.headers {
        if let Some(s) = v.as_str() {
            req = req.header(k, s);
        }
    }
    req
}

pub async fn health_check(
    endpoint: &ProviderEndpoint,
    credential: Option<&str>,
    http: &reqwest::Client,
) -> HealthStatus {
    if endpoint.discovery_path.is_none() && matches!(endpoint.protocol, Protocol::Custom) {
        return HealthStatus::DiscoveryUnsupported;
    }
    let url = discovery_url(endpoint);
    let resp = match request_builder(http, endpoint, credential, &url)
        .send()
        .await
    {
        Ok(r) => r,
        Err(_) => return HealthStatus::Unreachable,
    };
    match resp.status() {
        StatusCode::OK => HealthStatus::Healthy,
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => HealthStatus::AuthFailed,
        StatusCode::TOO_MANY_REQUESTS => HealthStatus::RateLimited,
        _ => HealthStatus::Unknown,
    }
}

pub async fn discover_models(
    endpoint: &ProviderEndpoint,
    credential: Option<&str>,
    http: &reqwest::Client,
) -> Result<Vec<ProviderModel>, ProviderError> {
    let url = discovery_url(endpoint);
    let resp = request_builder(http, endpoint, credential, &url)
        .send()
        .await
        .map_err(|_| ProviderError::Unreachable)?;
    match resp.status() {
        StatusCode::OK => {}
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => return Err(ProviderError::Auth),
        StatusCode::TOO_MANY_REQUESTS => return Err(ProviderError::RateLimit),
        _ => return Err(ProviderError::Unreachable),
    }
    let body: serde_json::Value = resp.json().await.map_err(|_| ProviderError::Malformed)?;
    let data = body
        .get("data")
        .and_then(|d| d.as_array())
        .ok_or(ProviderError::Malformed)?;
    let mut out = Vec::new();
    for item in data {
        if let Some(id) = item.get("id").and_then(|v| v.as_str()) {
            out.push(ProviderModel {
                id: id.to_string(),
                raw: item.clone(),
            });
        }
    }
    Ok(out)
}

impl HealthStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Healthy => "Healthy",
            Self::AuthFailed => "AuthFailed",
            Self::Unreachable => "Unreachable",
            Self::DiscoveryUnsupported => "DiscoveryUnsupported",
            Self::RateLimited => "RateLimited",
            Self::MalformedResponse => "MalformedResponse",
            Self::Unknown => "Unknown",
        }
    }
}
