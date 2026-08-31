//! Local application configuration and release feature flags.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const FEATURE_FLAGS_FILE: &str = "feature-flags.json";

/// Release flags are persisted in the app data directory so a dark-launched
/// feature can be enabled without rebuilding the desktop application.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FeatureFlags {
    /// Profiles and configuration sets are still experimental and disabled by
    /// default until their workflow is ready for general release.
    #[serde(default)]
    pub profiles_and_sets: bool,
}

impl Default for FeatureFlags {
    fn default() -> Self {
        Self {
            profiles_and_sets: false,
        }
    }
}

pub fn feature_flags_path() -> PathBuf {
    crate::app_data_dir().join(FEATURE_FLAGS_FILE)
}

/// Invalid or missing configuration fails closed. A malformed rollout file
/// must never accidentally expose an unreleased feature.
pub fn load_feature_flags() -> FeatureFlags {
    let path = feature_flags_path();
    match std::fs::read_to_string(&path) {
        Ok(contents) => match serde_json::from_str::<FeatureFlags>(&contents) {
            Ok(flags) => flags,
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "invalid feature flag configuration; using safe defaults");
                FeatureFlags::default()
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => FeatureFlags::default(),
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "could not read feature flag configuration; using safe defaults");
            FeatureFlags::default()
        }
    }
}

pub fn save_feature_flags(flags: &FeatureFlags) -> Result<(), String> {
    let path = feature_flags_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let contents = serde_json::to_string_pretty(flags).map_err(|error| error.to_string())? + "\n";
    chm_filesystem::atomic_write(&path, &contents).map_err(|error| error.to_string())
}

pub fn require_profiles_and_sets_enabled() -> Result<(), String> {
    if load_feature_flags().profiles_and_sets {
        Ok(())
    } else {
        Err(
            "Profiles and configuration sets are disabled by the profilesAndSets feature flag"
                .into(),
        )
    }
}

#[tauri::command]
pub async fn get_feature_flags_cmd() -> Result<FeatureFlags, String> {
    Ok(load_feature_flags())
}

#[tauri::command]
pub async fn set_feature_flags_cmd(flags: FeatureFlags) -> Result<(), String> {
    save_feature_flags(&flags)
}

#[cfg(test)]
mod tests {
    use super::FeatureFlags;

    #[test]
    fn missing_profiles_flag_defaults_closed() {
        let flags: FeatureFlags = serde_json::from_str("{}").unwrap();
        assert!(!flags.profiles_and_sets);
    }

    #[test]
    fn feature_flag_uses_camel_case_wire_name() {
        let flags = FeatureFlags {
            profiles_and_sets: true,
        };
        let value = serde_json::to_value(flags).unwrap();
        assert_eq!(value["profilesAndSets"], true);
    }
}
