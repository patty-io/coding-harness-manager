//! Provider and provider endpoint entities.
//! Rule: Provider != Endpoint != Model Route != Model Identity — never flattened.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::credentials::CredentialRef;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Provider {
    pub id: Uuid,
    pub name: String,
    pub display_name: String,
    pub enabled: bool,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Protocol {
    OpenAiChatCompletions,
    OpenAiResponses,
    AnthropicMessages,
    OpenRouterOpenAi,
    Custom,
}

impl Protocol {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OpenAiChatCompletions => "openai-chat",
            Self::OpenAiResponses => "openai-responses",
            Self::AnthropicMessages => "anthropic-messages",
            Self::OpenRouterOpenAi => "openrouter-openai",
            Self::Custom => "custom",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "openai-chat" => Self::OpenAiChatCompletions,
            "openai-responses" => Self::OpenAiResponses,
            "anthropic-messages" => Self::AnthropicMessages,
            "openrouter-openai" => Self::OpenRouterOpenAi,
            _ => Self::Custom,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuthType {
    None,
    ApiKeyHeader,
    BearerToken,
    CustomHeader,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderEndpoint {
    pub id: Uuid,
    pub provider_id: Uuid,
    pub name: String,
    pub base_url: String,
    pub protocol: Protocol,
    pub discovery_path: Option<String>,
    pub auth_type: AuthType,
    pub credential_ref: Option<CredentialRef>,
    pub headers: serde_json::Map<String, serde_json::Value>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
