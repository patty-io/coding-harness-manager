//! Apply-time deployment of credential references to harness-owned auth files.
//!
//! The database and `NativePlan` contain only `CredentialRef` metadata. This
//! module is the only sync path that receives a resolved secret, and it keeps
//! that value in an apply-scoped `ResolvedCredential`.

use chm_core::domain::credentials::{CredentialKind, CredentialRef};
use chm_harness_sdk::adapter::protected::{
    JsonAuthFormat, ProtectedChangePlan, ProtectedOperation, ProtectedTarget, ResolvedCredential,
};
use chm_filesystem::ProtectedWriteGuard;
use chm_secrets::SecretStore;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use thiserror::Error;
use toml_edit::{DocumentMut, Item, Table, value as toml_value};
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum DeploymentError {
    #[error("credential reference {credential_ref_id} is unavailable")]
    MissingCredential { credential_ref_id: Uuid },
    #[error("credential lookup failed: {0}")]
    SecretStore(String),
    #[error("protected target is invalid: {0}")]
    InvalidTarget(String),
    #[error("protected auth file is invalid: {0}")]
    InvalidAuth(String),
    #[error("protected write failed: {0}")]
    Filesystem(String),
}

pub struct PreparedChange {
    plan: ProtectedChangePlan,
    credential: ResolvedCredential,
    guard: ProtectedWriteGuard,
}

pub struct AppliedFile {
    guard: ProtectedWriteGuard,
}

impl AppliedFile {
    /// The path is safe to expose in an apply report; the guard never exposes
    /// the credential bytes it protects.
    pub fn path(&self) -> &std::path::Path {
        self.guard.path()
    }
}

/// Resolve all credential references and capture every protected target before
/// any ordinary native file is changed.
pub fn preflight(
    plans: &[ProtectedChangePlan],
    refs: &HashMap<Uuid, CredentialRef>,
    secrets: &dyn SecretStore,
) -> Result<Vec<PreparedChange>, DeploymentError> {
    let mut prepared = Vec::with_capacity(plans.len());
    for plan in plans {
        if !matches!(plan.operation, ProtectedOperation::Upsert) {
            return Err(DeploymentError::InvalidTarget(
                "credential removal is not supported by this sync path".into(),
            ));
        }
        let credential_ref = refs.get(&plan.credential_ref_id).ok_or(
            DeploymentError::MissingCredential {
                credential_ref_id: plan.credential_ref_id,
            },
        )?;
        let value = match credential_ref.kind {
            CredentialKind::Env => std::env::var(&credential_ref.reference).ok(),
            _ => secrets
                .get(&credential_ref.reference)
                .map_err(|error| DeploymentError::SecretStore(error.to_string()))?,
        }
        .ok_or(DeploymentError::MissingCredential {
            credential_ref_id: plan.credential_ref_id,
        })?;
        let path = match &plan.target {
            ProtectedTarget::JsonAuthFile { path, .. }
            | ProtectedTarget::EnvFile { path, .. }
            | ProtectedTarget::GooseSecretsFile { path, .. }
            | ProtectedTarget::KimiTomlFile { path, .. }
            | ProtectedTarget::KimiJsonFile { path, .. }
            | ProtectedTarget::NativeCredentialFile { path, .. } => PathBuf::from(path),
        };
        let guard = ProtectedWriteGuard::capture(&path)
            .map_err(|error| DeploymentError::Filesystem(error.to_string()))?;
        prepared.push(PreparedChange {
            plan: plan.clone(),
            credential: ResolvedCredential::new(value),
            guard,
        });
    }
    Ok(prepared)
}

fn target_path(target: &ProtectedTarget) -> &str {
    match target {
        ProtectedTarget::JsonAuthFile { path, .. }
        | ProtectedTarget::EnvFile { path, .. }
        | ProtectedTarget::GooseSecretsFile { path, .. }
        | ProtectedTarget::KimiTomlFile { path, .. }
        | ProtectedTarget::KimiJsonFile { path, .. }
        | ProtectedTarget::NativeCredentialFile { path, .. } => path,
    }
}

/// Check all protected targets immediately before ordinary native writes.
pub fn verify_preflight(prepared: &[PreparedChange]) -> Result<(), DeploymentError> {
    for change in prepared {
        change
            .guard
            .verify_unchanged()
            .map_err(|error| DeploymentError::Filesystem(error.to_string()))?;
    }
    Ok(())
}

/// Re-capture protected guards for targets that are also ordinary native
/// changes. This is needed for formats such as Kimi TOML where provider/model
/// metadata and the credential field share one file: ordinary metadata is
/// written first, then the credential is merged into that resulting document.
pub fn rebase_for_native_changes(
    prepared: &mut [PreparedChange],
    native_changes: &[chm_harness_sdk::adapter::types::NativeChange],
) -> Result<(), DeploymentError> {
    for change in prepared {
        if native_changes
            .iter()
            .any(|native| native.file_path == target_path(&change.plan.target))
        {
            let path = PathBuf::from(target_path(&change.plan.target));
            change.guard = ProtectedWriteGuard::capture(&path)
                .map_err(|error| DeploymentError::Filesystem(error.to_string()))?;
        }
    }
    Ok(())
}

fn upsert_env_value(raw: &str, key: &str, value: &str) -> String {
    let encoded = if value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || "_./:+-".contains(character))
    {
        value.to_string()
    } else {
        format!(
            "\"{}\"",
            value
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n")
                .replace('\r', "\\r")
        )
    };
    let prefix = format!("{key}=");
    let mut lines = raw.lines().map(str::to_string).collect::<Vec<_>>();
    if let Some(line) = lines.iter_mut().find(|line| {
        let trimmed = line.trim_start();
        !trimmed.starts_with('#') && trimmed.starts_with(&prefix)
    }) {
        *line = format!("{key}={encoded}");
    } else {
        lines.push(format!("{key}={encoded}"));
    }
    let mut output = lines.join("\n");
    if raw.ends_with('\n') || !output.is_empty() {
        output.push('\n');
    }
    output
}

/// Merge all provider entries for each auth file and atomically replace it.
/// Multiple providers sharing one auth file are written as one protected
/// change so the second provider cannot trip the first provider's concurrency
/// guard.
pub fn apply(prepared: &[PreparedChange]) -> Result<Vec<AppliedFile>, DeploymentError> {
    let mut by_path: HashMap<PathBuf, Vec<&PreparedChange>> = HashMap::new();
    for change in prepared {
        let path = match &change.plan.target {
            ProtectedTarget::JsonAuthFile { path, .. }
            | ProtectedTarget::EnvFile { path, .. }
            | ProtectedTarget::GooseSecretsFile { path, .. }
            | ProtectedTarget::KimiTomlFile { path, .. }
            | ProtectedTarget::KimiJsonFile { path, .. }
            | ProtectedTarget::NativeCredentialFile { path, .. } => PathBuf::from(path),
        };
        by_path.entry(path).or_default().push(change);
    }

    let mut applied = Vec::new();
    for (path, changes) in by_path {
        let first = changes
            .first()
            .ok_or_else(|| DeploymentError::InvalidTarget("empty protected group".into()))?;
        let guard = first.guard.clone();
        if changes
            .iter()
            .all(|change| matches!(change.plan.target, ProtectedTarget::KimiTomlFile { .. }))
        {
            let raw = match std::fs::read_to_string(&path) {
                Ok(raw) => raw,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
                Err(error) => {
                    let _ = rollback(&mut applied);
                    return Err(DeploymentError::Filesystem(format!(
                        "{}: {error}",
                        path.display()
                    )));
                }
            };
            let mut doc: DocumentMut = raw.parse().map_err(|error| {
                let _ = rollback(&mut applied);
                DeploymentError::InvalidAuth(format!("{}: {error}", path.display()))
            })?;
            let providers = doc
                .entry("providers")
                .or_insert(Item::Table(Table::new()))
                .as_table_mut()
                .ok_or_else(|| {
                    let _ = rollback(&mut applied);
                    DeploymentError::InvalidAuth(format!(
                        "{} providers must be a TOML table",
                        path.display()
                    ))
                })?;
            for change in changes.iter() {
                let ProtectedTarget::KimiTomlFile { provider_id, .. } = &change.plan.target else {
                    unreachable!("Kimi TOML group was checked above")
                };
                let provider = providers
                    .entry(provider_id)
                    .or_insert(Item::Table(Table::new()))
                    .as_table_mut()
                    .ok_or_else(|| {
                        DeploymentError::InvalidAuth(format!(
                            "{} provider {provider_id} must be a TOML table",
                            path.display()
                        ))
                    })?;
                provider["api_key"] = toml_value(change.credential.expose());
            }
            if let Err(error) = guard.replace(doc.to_string().as_bytes(), 0o600) {
                let _ = rollback(&mut applied);
                return Err(DeploymentError::Filesystem(error.to_string()));
            }
            applied.push(AppliedFile { guard });
            continue;
        }
        if changes
            .iter()
            .all(|change| matches!(change.plan.target, ProtectedTarget::KimiJsonFile { .. }))
        {
            let raw = match std::fs::read_to_string(&path) {
                Ok(raw) => raw,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => "{}".into(),
                Err(error) => {
                    let _ = rollback(&mut applied);
                    return Err(DeploymentError::Filesystem(format!(
                        "{}: {error}",
                        path.display()
                    )));
                }
            };
            let mut doc: Value = serde_json::from_str(&raw).map_err(|error| {
                let _ = rollback(&mut applied);
                DeploymentError::InvalidAuth(format!("{}: {error}", path.display()))
            })?;
            let providers = doc.as_object_mut().ok_or_else(|| {
                let _ = rollback(&mut applied);
                DeploymentError::InvalidAuth(format!("{} must contain a JSON object", path.display()))
            })?
            .entry("providers")
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .ok_or_else(|| {
                let _ = rollback(&mut applied);
                DeploymentError::InvalidAuth(format!("{} providers must be an object", path.display()))
            })?;
            for change in changes.iter() {
                let ProtectedTarget::KimiJsonFile { provider_id, .. } = &change.plan.target else {
                    unreachable!("Kimi JSON group was checked above")
                };
                let provider = providers
                    .entry(provider_id.clone())
                    .or_insert_with(|| Value::Object(Map::new()))
                    .as_object_mut()
                    .ok_or_else(|| {
                        DeploymentError::InvalidAuth(format!(
                            "{} provider {provider_id} must be a JSON object",
                            path.display()
                        ))
                    })?;
                provider.insert("api_key".into(), Value::String(change.credential.expose().into()));
            }
            let serialized = serde_json::to_vec_pretty(&doc).map_err(|error| {
                let _ = rollback(&mut applied);
                DeploymentError::InvalidAuth(error.to_string())
            })?;
            if let Err(error) = guard.replace(&serialized, 0o600) {
                let _ = rollback(&mut applied);
                return Err(DeploymentError::Filesystem(error.to_string()));
            }
            applied.push(AppliedFile { guard });
            continue;
        }
        if changes.iter().any(|change| {
            matches!(
                change.plan.target,
                ProtectedTarget::KimiTomlFile { .. } | ProtectedTarget::KimiJsonFile { .. }
            )
        }) {
            let _ = rollback(&mut applied);
            return Err(DeploymentError::InvalidTarget(format!(
                "protected targets for {} use incompatible formats",
                path.display()
            )));
        }
        if changes
            .iter()
            .all(|change| matches!(change.plan.target, ProtectedTarget::GooseSecretsFile { .. }))
        {
            let raw = match std::fs::read_to_string(&path) {
                Ok(raw) => raw,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
                Err(error) => {
                    let _ = rollback(&mut applied);
                    return Err(DeploymentError::Filesystem(format!(
                        "{}: {error}",
                        path.display()
                    )));
                }
            };
            let mut doc: serde_yaml::Value = if raw.trim().is_empty() {
                serde_yaml::Value::Mapping(serde_yaml::Mapping::new())
            } else {
                serde_yaml::from_str(&raw).map_err(|error| {
                    let _ = rollback(&mut applied);
                    DeploymentError::InvalidAuth(format!("{}: {error}", path.display()))
                })?
            };
            let root = doc.as_mapping_mut().ok_or_else(|| {
                let _ = rollback(&mut applied);
                DeploymentError::InvalidAuth(format!(
                    "{} must contain a YAML mapping",
                    path.display()
                ))
            })?;
            for change in changes.iter() {
                let ProtectedTarget::GooseSecretsFile { key, .. } = &change.plan.target else {
                    unreachable!("Goose secrets group was checked above")
                };
                if key.trim().is_empty() {
                    let _ = rollback(&mut applied);
                    return Err(DeploymentError::InvalidTarget(format!(
                        "{} has an empty secret key",
                        path.display()
                    )));
                }
                root.insert(
                    serde_yaml::Value::String(key.clone()),
                    serde_yaml::Value::String(change.credential.expose().into()),
                );
            }
            let serialized = serde_yaml::to_string(&doc).map_err(|error| {
                let _ = rollback(&mut applied);
                DeploymentError::InvalidAuth(error.to_string())
            })?;
            if let Err(error) = guard.replace(serialized.as_bytes(), 0o600) {
                let _ = rollback(&mut applied);
                return Err(DeploymentError::Filesystem(error.to_string()));
            }
            applied.push(AppliedFile { guard });
            continue;
        }
        if changes.iter().any(|change| {
            matches!(change.plan.target, ProtectedTarget::GooseSecretsFile { .. })
        }) {
            let _ = rollback(&mut applied);
            return Err(DeploymentError::InvalidTarget(format!(
                "protected targets for {} use incompatible formats",
                path.display()
            )));
        }
        if changes
            .iter()
            .all(|change| matches!(change.plan.target, ProtectedTarget::EnvFile { .. }))
        {
            let raw = match std::fs::read_to_string(&path) {
                Ok(raw) => raw,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
                Err(error) => {
                    let _ = rollback(&mut applied);
                    return Err(DeploymentError::Filesystem(format!(
                        "{}: {error}",
                        path.display()
                    )));
                }
            };
            let mut updated = raw;
            for change in changes.iter() {
                let ProtectedTarget::EnvFile { key, .. } = &change.plan.target else {
                    unreachable!("env group was checked above")
                };
                if key.trim().is_empty() {
                    let _ = rollback(&mut applied);
                    return Err(DeploymentError::InvalidTarget(format!(
                        "{} has an empty environment key",
                        path.display()
                    )));
                }
                updated = upsert_env_value(&updated, key, change.credential.expose());
            }
            if let Err(error) = guard.replace(updated.as_bytes(), 0o600) {
                let _ = rollback(&mut applied);
                return Err(DeploymentError::Filesystem(error.to_string()));
            }
            applied.push(AppliedFile { guard });
            continue;
        }
        if changes
            .iter()
            .any(|change| matches!(change.plan.target, ProtectedTarget::EnvFile { .. }))
        {
            let _ = rollback(&mut applied);
            return Err(DeploymentError::InvalidTarget(format!(
                "protected targets for {} use incompatible formats",
                path.display()
            )));
        }
        let raw = match std::fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => "{}".into(),
            Err(error) => {
                let _ = rollback(&mut applied);
                return Err(DeploymentError::Filesystem(format!(
                    "{}: {error}",
                    path.display()
                )));
            }
        };
        let mut doc: Value = serde_json::from_str(&raw)
            .map_err(|error| {
                let _ = rollback(&mut applied);
                DeploymentError::InvalidAuth(format!("{}: {error}", path.display()))
            })?;
        let root = doc.as_object_mut().ok_or_else(|| {
            let _ = rollback(&mut applied);
            DeploymentError::InvalidAuth(format!("{} must contain a JSON object", path.display()))
        })?;
        for change in changes.iter() {
            let (provider_id, format) = match &change.plan.target {
                ProtectedTarget::JsonAuthFile {
                    provider_id, format, ..
                } => (provider_id, format),
                ProtectedTarget::EnvFile { .. } => {
                    let _ = rollback(&mut applied);
                    return Err(DeploymentError::InvalidTarget(format!(
                        "protected target {} mixes dotenv and JSON auth formats",
                        path.display()
                    )));
                }
                ProtectedTarget::GooseSecretsFile { .. } => {
                    let _ = rollback(&mut applied);
                    return Err(DeploymentError::InvalidTarget(format!(
                        "protected target {} mixes Goose secrets and JSON auth formats",
                        path.display()
                    )));
                }
                ProtectedTarget::KimiTomlFile { .. } | ProtectedTarget::KimiJsonFile { .. } => {
                    let _ = rollback(&mut applied);
                    return Err(DeploymentError::InvalidTarget(format!(
                        "protected target {} mixes Kimi config and JSON auth formats",
                        path.display()
                    )));
                }
                ProtectedTarget::NativeCredentialFile { provider_id, .. } => {
                    let _ = rollback(&mut applied);
                    return Err(DeploymentError::InvalidTarget(format!(
                        "native credential target {} has no writer",
                        provider_id
                    )))
                }
            };
            let entry = root
                .entry(provider_id.clone())
                .or_insert_with(|| Value::Object(Map::new()))
                .as_object_mut()
                .ok_or_else(|| {
                    let _ = rollback(&mut applied);
                    DeploymentError::InvalidAuth(format!(
                        "{} provider {} must be a JSON object",
                        path.display(),
                        provider_id
                    ))
                })?;
            // Preserve provider-scoped metadata (for example Pi's `env`
            // values or an existing refresh token) while replacing only the
            // credential fields CHM owns.
            entry.insert(
                "type".into(),
                Value::String(match format {
                    JsonAuthFormat::OpenCode => "api",
                    JsonAuthFormat::Pi => "api_key",
                }
                .into()),
            );
            entry.insert(
                "key".into(),
                Value::String(change.credential.expose().into()),
            );
        }
        let serialized = serde_json::to_vec_pretty(&doc)
            .map_err(|error| {
                let _ = rollback(&mut applied);
                DeploymentError::InvalidAuth(error.to_string())
            })?;
        if let Err(error) = guard.replace(&serialized, 0o600) {
            let _ = rollback(&mut applied);
            return Err(DeploymentError::Filesystem(error.to_string()));
        }
        applied.push(AppliedFile {
            guard,
        });
    }
    Ok(applied)
}

pub fn rollback(applied: &mut Vec<AppliedFile>) -> Result<(), DeploymentError> {
    for file in applied.drain(..).rev() {
        file.guard
            .restore()
            .map_err(|error| DeploymentError::Filesystem(error.to_string()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chm_core::domain::credentials::CredentialKind;
    use tempfile::TempDir;

    struct FakeSecrets;
    impl SecretStore for FakeSecrets {
        fn set(&self, _: &str, _: &str) -> Result<(), chm_secrets::SecretError> {
            Ok(())
        }
        fn get(&self, key: &str) -> Result<Option<String>, chm_secrets::SecretError> {
            Ok((key == "first").then(|| "secret-one".into()))
        }
        fn delete(&self, _: &str) -> Result<(), chm_secrets::SecretError> {
            Ok(())
        }
    }

    fn reference(id: Uuid, name: &str) -> CredentialRef {
        CredentialRef {
            id,
            kind: CredentialKind::Keychain,
            reference: name.into(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn missing_second_secret_prevents_first_write() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("auth.json");
        let first = Uuid::new_v4();
        let missing = Uuid::new_v4();
        let plans = vec![
            ProtectedChangePlan {
                target: ProtectedTarget::JsonAuthFile {
                    path: path.display().to_string(),
                    provider_id: "first".into(),
                    format: JsonAuthFormat::OpenCode,
                },
                credential_ref_id: first,
                operation: ProtectedOperation::Upsert,
            },
            ProtectedChangePlan {
                target: ProtectedTarget::JsonAuthFile {
                    path: path.display().to_string(),
                    provider_id: "missing".into(),
                    format: JsonAuthFormat::OpenCode,
                },
                credential_ref_id: missing,
                operation: ProtectedOperation::Upsert,
            },
        ];
        let refs = HashMap::from([(first, reference(first, "first"))]);
        assert!(matches!(
            preflight(&plans, &refs, &FakeSecrets),
            Err(DeploymentError::MissingCredential { credential_ref_id }) if credential_ref_id == missing
        ));
        assert!(!path.exists());
    }

    #[test]
    fn env_file_merge_preserves_unrelated_values_and_updates_one_key() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(".env");
        std::fs::write(&path, "# keep\nOTHER=value\nCHM_KEY=old\n").unwrap();
        let id = Uuid::new_v4();
        let plans = vec![ProtectedChangePlan {
            target: ProtectedTarget::EnvFile {
                path: path.display().to_string(),
                key: "CHM_KEY".into(),
            },
            credential_ref_id: id,
            operation: ProtectedOperation::Upsert,
        }];
        let refs = HashMap::from([(id, reference(id, "first"))]);
        let prepared = preflight(&plans, &refs, &FakeSecrets).unwrap();
        let applied = apply(&prepared).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("# keep"));
        assert!(content.contains("OTHER=value"));
        assert!(content.contains("CHM_KEY=secret-one"));
        assert!(!content.contains("CHM_KEY=old"));
        let mut applied = applied;
        rollback(&mut applied).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "# keep\nOTHER=value\nCHM_KEY=old\n"
        );
    }

    #[test]
    fn json_auth_merge_preserves_provider_metadata() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("auth.json");
        std::fs::write(
            &path,
            r#"{
  "proxy": {
    "type": "api",
    "key": "old",
    "env": {"REGION": "us-east-1"},
    "refresh": true
  },
  "other": {"type": "api", "key": "untouched"}
}"#,
        )
        .unwrap();
        let id = Uuid::new_v4();
        let plans = vec![ProtectedChangePlan {
            target: ProtectedTarget::JsonAuthFile {
                path: path.display().to_string(),
                provider_id: "proxy".into(),
                format: JsonAuthFormat::Pi,
            },
            credential_ref_id: id,
            operation: ProtectedOperation::Upsert,
        }];
        let refs = HashMap::from([(id, reference(id, "first"))]);
        let prepared = preflight(&plans, &refs, &FakeSecrets).unwrap();
        let mut applied = apply(&prepared).unwrap();
        let parsed: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(parsed["proxy"]["type"].as_str(), Some("api_key"));
        assert_eq!(parsed["proxy"]["key"].as_str(), Some("secret-one"));
        assert_eq!(parsed["proxy"]["env"]["REGION"].as_str(), Some("us-east-1"));
        assert_eq!(parsed["proxy"]["refresh"].as_bool(), Some(true));
        assert_eq!(parsed["other"]["key"].as_str(), Some("untouched"));
        rollback(&mut applied).unwrap();
        assert!(std::fs::read_to_string(&path).unwrap().contains("\"old\""));
    }

    #[test]
    fn goose_secrets_merge_is_yaml_and_rolls_back() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("secrets.yaml");
        std::fs::write(&path, "OTHER: keep\n").unwrap();
        let id = Uuid::new_v4();
        let plans = vec![ProtectedChangePlan {
            target: ProtectedTarget::GooseSecretsFile {
                path: path.display().to_string(),
                key: "CHM_YOLO_API_KEY".into(),
            },
            credential_ref_id: id,
            operation: ProtectedOperation::Upsert,
        }];
        let refs = HashMap::from([(id, reference(id, "first"))]);
        let prepared = preflight(&plans, &refs, &FakeSecrets).unwrap();
        let mut applied = apply(&prepared).unwrap();
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(parsed["OTHER"].as_str(), Some("keep"));
        assert_eq!(parsed["CHM_YOLO_API_KEY"].as_str(), Some("secret-one"));
        rollback(&mut applied).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "OTHER: keep\n");
    }
}
