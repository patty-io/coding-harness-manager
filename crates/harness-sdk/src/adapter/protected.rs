//! Secret-free descriptions of credential changes.
//!
//! Native plans may be serialized for previews, hashes, and audit records, so
//! they can contain only references and target metadata. Resolved secret
//! values live in `ResolvedCredential` for the duration of an apply call.

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JsonAuthFormat {
    OpenCode,
    Pi,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProtectedTarget {
    /// A harness-owned JSON auth file with a provider-keyed API credential.
    JsonAuthFile {
        path: String,
        provider_id: String,
        format: JsonAuthFormat,
    },
    /// A harness-owned dotenv file. The key is metadata only; the value is
    /// resolved and written by the desktop coordinator at apply time.
    EnvFile { path: String, key: String },
    /// Goose's file-backed secret store. Goose reads this YAML map when its
    /// keyring is disabled or unavailable; the adapter also writes the
    /// corresponding `GOOSE_DISABLE_KEYRING` setting when it needs this
    /// portable path.
    GooseSecretsFile { path: String, key: String },
    /// Kimi Code's documented provider credential field in a TOML config.
    KimiTomlFile { path: String, provider_id: String },
    /// Kimi Code installations that use the JSON equivalent of config.toml.
    KimiJsonFile { path: String, provider_id: String },
    /// A harness-owned protected file whose adapter-specific writer consumes
    /// the value at apply time. The descriptor itself remains serializable.
    NativeCredentialFile { path: String, provider_id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProtectedOperation {
    Upsert,
    Remove,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtectedChangePlan {
    pub target: ProtectedTarget,
    pub credential_ref_id: Uuid,
    pub operation: ProtectedOperation,
}

/// A resolved credential deliberately has no `Debug`, `Clone`, or serde
/// implementations. Callers can expose it only at the point where a native
/// protected writer needs the value.
pub struct ResolvedCredential(Zeroizing<String>);

impl ResolvedCredential {
    pub fn new(value: String) -> Self {
        Self(Zeroizing::new(value))
    }

    pub fn expose(&self) -> &str {
        self.0.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_plan_serializes_reference_but_cannot_hold_value() {
        let plan = ProtectedChangePlan {
            target: ProtectedTarget::JsonAuthFile {
                path: "/tmp/auth.json".into(),
                provider_id: "yolo-auto".into(),
                format: JsonAuthFormat::OpenCode,
            },
            credential_ref_id: Uuid::nil(),
            operation: ProtectedOperation::Upsert,
        };
        let json = serde_json::to_string(&plan).unwrap();
        assert!(json.contains("yolo-auto"));
        assert!(!json.contains("api_key"));
    }

    #[test]
    fn resolved_credential_exposes_only_an_apply_scoped_view() {
        let credential = ResolvedCredential::new("secret-value".into());
        assert_eq!(credential.expose(), "secret-value");
    }
}
