use chm_core::domain::credentials::{CredentialKind, CredentialRef};
use chm_core::domain::provider::{AuthType, Protocol, Provider, ProviderEndpoint};
use chrono::{TimeZone, Utc};
use uuid::Uuid;

fn ts() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 27, 9, 0, 0).unwrap()
}

#[test]
fn provider_roundtrips_through_json() {
    let p = Provider {
        id: Uuid::new_v4(),
        name: "zai".into(),
        display_name: "Z.AI".into(),
        enabled: true,
        notes: None,
        created_at: ts(),
        updated_at: ts(),
    };
    let json = serde_json::to_string(&p).unwrap();
    let back: Provider = serde_json::from_str(&json).unwrap();
    assert_eq!(p, back);
}

#[test]
fn endpoint_roundtrips_with_credential_ref() {
    let e = ProviderEndpoint {
        id: Uuid::new_v4(),
        provider_id: Uuid::new_v4(),
        name: "Anthropic-compatible".into(),
        base_url: "https://api.z.ai/api/anthropic".into(),
        protocol: Protocol::AnthropicMessages,
        discovery_path: Some("/v1/models".into()),
        auth_type: AuthType::BearerToken,
        credential_ref: Some(CredentialRef {
            id: Uuid::new_v4(),
            kind: CredentialKind::Keychain,
            reference: "coding-harness-manager/providers/abc".into(),
            created_at: ts(),
            updated_at: ts(),
        }),
        headers: serde_json::Map::new(),
        enabled: true,
        created_at: ts(),
        updated_at: ts(),
    };
    let json = serde_json::to_string(&e).unwrap();
    let back: ProviderEndpoint = serde_json::from_str(&json).unwrap();
    assert_eq!(e, back);
    assert_eq!(back.credential_ref.unwrap().kind, CredentialKind::Keychain);
}

#[test]
fn protocol_and_status_strings_roundtrip() {
    assert_eq!(
        Protocol::from_str(Protocol::OpenAiResponses.as_str()),
        Protocol::OpenAiResponses
    );
    assert_eq!(Protocol::from_str("garbage"), Protocol::Custom);
    assert_eq!(
        CredentialKind::from_str(CredentialKind::Libsecret.as_str()),
        CredentialKind::Libsecret
    );
}
