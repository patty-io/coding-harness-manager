//! Secret-free provider-route deployment contract shared by every adapter.

use chm_core::domain::credentials::CredentialRef;
use chm_core::domain::models::ModelRoute;
use chm_core::domain::provider::{AuthType, Protocol};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderTopology {
    Multiple,
    SingleGlobalOverride,
    FixedProvider { provider_id: String },
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CredentialTarget {
    NativeSecretStore,
    CommandHelper,
    HarnessEnvFile,
    ProtectedConfig,
    ManagedRemoteApi,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CredentialRequirement {
    None,
    Secret {
        credential_ref: CredentialRef,
        auth_type: AuthType,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderRouteBundle {
    pub provider_id: String,
    pub display_name: String,
    pub endpoint_id: Uuid,
    pub base_url: String,
    pub protocol: Protocol,
    pub credential: CredentialRequirement,
    pub models: Vec<ModelRoute>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelIdentityRules {
    pub case_sensitive: bool,
    pub allow_namespaced_ids: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelMetadataCapabilities {
    pub context_window: bool,
    pub max_input: bool,
    pub max_output: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteDeploymentCapabilities {
    pub provider_topology: ProviderTopology,
    pub protocols: Vec<Protocol>,
    pub credential_targets: Vec<CredentialTarget>,
    pub model_identity: ModelIdentityRules,
    pub metadata: ModelMetadataCapabilities,
}

impl RouteDeploymentCapabilities {
    pub fn unsupported() -> Self {
        Self {
            provider_topology: ProviderTopology::None,
            protocols: Vec::new(),
            credential_targets: Vec::new(),
            model_identity: ModelIdentityRules {
                case_sensitive: true,
                allow_namespaced_ids: false,
            },
            metadata: ModelMetadataCapabilities {
                context_window: false,
                max_input: false,
                max_output: false,
            },
        }
    }

    pub fn check(&self, bundle: &ProviderRouteBundle) -> RouteCompatibility {
        if bundle.provider_id.trim().is_empty() {
            return RouteCompatibility::Blocked {
                reason: "the provider identity is empty".into(),
            };
        }
        match &self.provider_topology {
            ProviderTopology::None => {
                return RouteCompatibility::Blocked {
                    reason: "this harness cannot deploy provider routes".into(),
                };
            }
            ProviderTopology::FixedProvider { provider_id }
                if !provider_id.eq_ignore_ascii_case(&bundle.provider_id) =>
            {
                return RouteCompatibility::Blocked {
                    reason: format!("this harness is fixed to provider {provider_id}"),
                };
            }
            ProviderTopology::Multiple
            | ProviderTopology::SingleGlobalOverride
            | ProviderTopology::FixedProvider { .. } => {}
        }
        if !self.protocols.contains(&bundle.protocol) {
            return RouteCompatibility::Blocked {
                reason: format!(
                    "{} is not supported by this harness",
                    protocol_display_name(bundle.protocol)
                ),
            };
        }
        if matches!(bundle.credential, CredentialRequirement::Secret { .. })
            && self.credential_targets.is_empty()
        {
            return RouteCompatibility::Blocked {
                reason: "this harness has no writable credential target".into(),
            };
        }
        if !self.model_identity.allow_namespaced_ids
            && bundle
                .models
                .iter()
                .any(|model| model.remote_model_id.contains('/'))
        {
            return RouteCompatibility::Blocked {
                reason: "this harness cannot represent namespaced model IDs".into(),
            };
        }
        if !self.metadata.context_window
            && bundle
                .models
                .iter()
                .any(|model| model.context_window.is_some())
        {
            return RouteCompatibility::Blocked {
                reason: "this harness cannot preserve model context windows".into(),
            };
        }
        if !self.metadata.max_input && bundle.models.iter().any(|model| model.max_input.is_some()) {
            return RouteCompatibility::Blocked {
                reason: "this harness cannot preserve model input limits".into(),
            };
        }
        if !self.metadata.max_output && bundle.models.iter().any(|model| model.max_output.is_some())
        {
            return RouteCompatibility::Blocked {
                reason: "this harness cannot preserve model output limits".into(),
            };
        }
        RouteCompatibility::Ready
    }
}

fn protocol_display_name(protocol: Protocol) -> &'static str {
    match protocol {
        Protocol::OpenAiChatCompletions => "OpenAI Chat Completions",
        Protocol::OpenAiResponses => "OpenAI Responses",
        Protocol::AnthropicMessages => "Anthropic Messages",
        Protocol::OpenRouterOpenAi => "OpenRouter OpenAI",
        Protocol::Custom => "Custom protocol",
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteCompatibility {
    Ready,
    Blocked { reason: String },
}
