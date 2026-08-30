//! OS-native secret storage. SQLite stores references, never values.

use std::path::PathBuf;
use std::process::Command;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SecretError {
    #[error("unsupported on this platform: {0}")]
    Unsupported(&'static str),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("keychain error: {0}")]
    Keychain(String),
    #[error("crypto error: {0}")]
    Crypto(String),
    #[error("secret not found: {0}")]
    NotFound(String),
}

pub trait SecretStore: Send + Sync {
    fn set(&self, key: &str, value: &str) -> Result<(), SecretError>;
    fn get(&self, key: &str) -> Result<Option<String>, SecretError>;
    fn delete(&self, key: &str) -> Result<(), SecretError>;
}

/// Reads secrets from the process environment. Env references are
/// user-managed, so set/delete are unsupported.
pub struct EnvStore;

impl SecretStore for EnvStore {
    fn set(&self, _key: &str, _value: &str) -> Result<(), SecretError> {
        Err(SecretError::Unsupported("env references are user-managed"))
    }

    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        Ok(std::env::var(key).ok())
    }

    fn delete(&self, _key: &str) -> Result<(), SecretError> {
        Err(SecretError::Unsupported("env references are user-managed"))
    }
}

/// macOS Keychain via the `security` CLI.
pub struct KeychainStore {
    service: String,
}

impl KeychainStore {
    pub fn new(service: &str) -> Self {
        Self {
            service: service.to_string(),
        }
    }

    fn account(&self, key: &str) -> String {
        // Credential references persist the Keychain service as a prefix
        // (for example, `coding-harness-manager/providers/<id>`), while the
        // macOS `security` command receives the account separately. Accept
        // both the persisted reference and the raw account key so existing
        // credentials can be resolved consistently.
        key.strip_prefix(&format!("{}/", self.service))
            .unwrap_or(key)
            .to_string()
    }

    fn account_candidates(&self, key: &str) -> Vec<String> {
        let account = self.account(key);
        if account == key {
            vec![account]
        } else {
            // Older versions stored the complete credential reference as the
            // account. Keep that form as a fallback so existing secrets are
            // not lost when the canonical account is normalized.
            vec![account, key.to_string()]
        }
    }
}

impl SecretStore for KeychainStore {
    fn set(&self, key: &str, value: &str) -> Result<(), SecretError> {
        let out = Command::new("security")
            .args([
                "add-generic-password",
                "-U",
                "-s",
                &self.service,
                "-a",
                &self.account(key),
                "-w",
                value,
            ])
            .output()?;
        if !out.status.success() {
            return Err(SecretError::Keychain(
                String::from_utf8_lossy(&out.stderr).into_owned(),
            ));
        }
        Ok(())
    }

    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        for account in self.account_candidates(key) {
            let out = Command::new("security")
                .args([
                    "find-generic-password",
                    "-s",
                    &self.service,
                    "-a",
                    &account,
                    "-w",
                ])
                .output()?;
            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr);
                if stderr.contains("could not be found") || stderr.contains("errSecItemNotFound") {
                    continue;
                }
                return Err(SecretError::Keychain(stderr.into_owned()));
            }
            return Ok(Some(
                String::from_utf8(out.stdout)
                    .map_err(|e| SecretError::Crypto(e.to_string()))?
                    .trim_end_matches('\n')
                    .to_string(),
            ));
        }
        Ok(None)
    }

    fn delete(&self, key: &str) -> Result<(), SecretError> {
        for account in self.account_candidates(key) {
            let out = Command::new("security")
                .args([
                    "delete-generic-password",
                    "-s",
                    &self.service,
                    "-a",
                    &account,
                ])
                .output()?;
            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr);
                if stderr.contains("could not be found") || stderr.contains("errSecItemNotFound") {
                    continue;
                }
                return Err(SecretError::Keychain(stderr.into_owned()));
            }
        }
        Ok(())
    }
}

/// Windows Credential Manager — implemented in Phase 14.
pub struct WindowsCredentialManagerStore;

/// Linux Secret Service (libsecret) — implemented in Phase 14.
pub struct LibsecretStore;

impl SecretStore for WindowsCredentialManagerStore {
    fn set(&self, _k: &str, _v: &str) -> Result<(), SecretError> {
        Err(SecretError::Unsupported(
            "windows credential manager lands in phase 14",
        ))
    }
    fn get(&self, _k: &str) -> Result<Option<String>, SecretError> {
        Err(SecretError::Unsupported(
            "windows credential manager lands in phase 14",
        ))
    }
    fn delete(&self, _k: &str) -> Result<(), SecretError> {
        Err(SecretError::Unsupported(
            "windows credential manager lands in phase 14",
        ))
    }
}

impl SecretStore for LibsecretStore {
    fn set(&self, _k: &str, _v: &str) -> Result<(), SecretError> {
        Err(SecretError::Unsupported("libsecret lands in phase 14"))
    }
    fn get(&self, _k: &str) -> Result<Option<String>, SecretError> {
        Err(SecretError::Unsupported("libsecret lands in phase 14"))
    }
    fn delete(&self, _k: &str) -> Result<(), SecretError> {
        Err(SecretError::Unsupported("libsecret lands in phase 14"))
    }
}

pub fn default_store() -> Box<dyn SecretStore> {
    #[cfg(target_os = "macos")]
    {
        Box::new(KeychainStore::new("coding-harness-manager"))
    }
    #[cfg(target_os = "windows")]
    {
        Box::new(WindowsCredentialManagerStore)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Box::new(LibsecretStore)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", unix)))]
    {
        Box::new(EncryptedVaultStore::new("chm-vault"))
    }
}

/// Fallback AES-GCM vault used only when no OS secret service exists.
/// Real crypto implementation lands in Phase 14; here it compiles behind the trait.
#[allow(dead_code)]
pub struct EncryptedVaultStore {
    path: PathBuf,
}

impl EncryptedVaultStore {
    pub fn new(name: &str) -> Self {
        let dir = dirs_data_dir();
        Self {
            path: dir.join(format!("{name}.json")),
        }
    }
}

fn dirs_data_dir() -> PathBuf {
    std::env::var_os("CHM_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME").unwrap_or_default();
            PathBuf::from(home).join(".coding-harness-manager")
        })
}

#[cfg(test)]
mod tests {
    use super::KeychainStore;

    #[test]
    fn keychain_reference_prefix_is_not_part_of_account() {
        let store = KeychainStore::new("coding-harness-manager");

        assert_eq!(
            store.account("coding-harness-manager/providers/harness/example"),
            "providers/harness/example"
        );
        assert_eq!(
            store.account("providers/harness/example"),
            "providers/harness/example"
        );
    }

    #[test]
    fn keychain_reference_candidates_include_legacy_prefixed_account() {
        let store = KeychainStore::new("coding-harness-manager");

        assert_eq!(
            store.account_candidates("coding-harness-manager/providers/zai"),
            vec![
                "providers/zai".to_string(),
                "coding-harness-manager/providers/zai".to_string()
            ]
        );
        assert_eq!(
            store.account_candidates("providers/zai"),
            vec!["providers/zai".to_string()]
        );
    }
}
