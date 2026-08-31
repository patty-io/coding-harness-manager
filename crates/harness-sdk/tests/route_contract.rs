use chm_core::domain::credentials::{CredentialKind, CredentialRef};
use chm_core::domain::models::ModelRoute;
use chm_core::domain::provider::{AuthType, Protocol};
use chm_harness_sdk::adapter::route::{
    CredentialRequirement, CredentialTarget, ModelIdentityRules, ModelMetadataCapabilities,
    ProviderRouteBundle, ProviderTopology, RouteCompatibility, RouteDeploymentCapabilities,
};
use chrono::Utc;
use uuid::Uuid;

fn credential() -> CredentialRef {
    CredentialRef {
        id: Uuid::new_v4(),
        kind: CredentialKind::Keychain,
        reference: "coding-harness-manager/providers/yolo-auto".into(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

fn bundle(protocol: Protocol) -> ProviderRouteBundle {
    let endpoint_id = Uuid::new_v4();
    let mut model = ModelRoute::new(
        "qwen3.8-27b".into(),
        "Qwen 3.8 27B".into(),
        Some(131_072),
        serde_json::json!({}),
        serde_json::json!({}),
    );
    model.endpoint_id = endpoint_id;
    ProviderRouteBundle {
        provider_id: "yolo-auto".into(),
        display_name: "Yolo-Auto".into(),
        endpoint_id,
        base_url: "https://yolo-auto.com/v1".into(),
        protocol,
        credential: CredentialRequirement::Secret {
            credential_ref: credential(),
            auth_type: AuthType::BearerToken,
        },
        models: vec![model],
    }
}

fn capabilities(
    topology: ProviderTopology,
    protocols: Vec<Protocol>,
) -> RouteDeploymentCapabilities {
    RouteDeploymentCapabilities {
        provider_topology: topology,
        protocols,
        credential_targets: vec![CredentialTarget::NativeSecretStore],
        model_identity: ModelIdentityRules {
            case_sensitive: true,
            allow_namespaced_ids: true,
        },
        metadata: ModelMetadataCapabilities {
            context_window: true,
            max_input: true,
            max_output: true,
        },
    }
}

#[test]
fn matching_route_is_ready() {
    let caps = capabilities(
        ProviderTopology::Multiple,
        vec![Protocol::OpenAiChatCompletions],
    );
    assert_eq!(
        caps.check(&bundle(Protocol::OpenAiChatCompletions)),
        RouteCompatibility::Ready
    );
}

#[test]
fn rejects_protocol_and_topology_mismatches() {
    let responses_only = capabilities(ProviderTopology::Multiple, vec![Protocol::OpenAiResponses]);
    assert_eq!(
        responses_only.check(&bundle(Protocol::OpenAiChatCompletions)),
        RouteCompatibility::Blocked {
            reason: "OpenAI Chat Completions is not supported by this harness".into(),
        }
    );

    let fixed = capabilities(
        ProviderTopology::FixedProvider {
            provider_id: "anthropic".into(),
        },
        vec![Protocol::OpenAiChatCompletions],
    );
    assert_eq!(
        fixed.check(&bundle(Protocol::OpenAiChatCompletions)),
        RouteCompatibility::Blocked {
            reason: "this harness is fixed to provider anthropic".into(),
        }
    );
}

#[test]
fn unauthenticated_route_does_not_require_a_credential_target() {
    let mut route = bundle(Protocol::OpenAiChatCompletions);
    route.credential = CredentialRequirement::None;
    let mut caps = capabilities(
        ProviderTopology::Multiple,
        vec![Protocol::OpenAiChatCompletions],
    );
    caps.credential_targets.clear();
    assert_eq!(caps.check(&route), RouteCompatibility::Ready);
}
