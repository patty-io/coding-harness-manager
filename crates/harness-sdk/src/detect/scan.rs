//! Orchestrates full-machine detection into a normalized inventory.

use std::path::Path;

use chm_core::domain::harness::{HarnessInstallation, HarnessType, InstallationStatus};
use chrono::Utc;
use uuid::Uuid;

use crate::definition::Platform;
use crate::definition::all_definitions;

use super::paths::{find_executable, home_dir, resolve_config_path};
use super::version::{detect_version, version_args_for};

#[derive(Debug, Default)]
pub struct HarnessInventory {
    pub installations: Vec<HarnessInstallation>,
}

pub fn scan(platform: Platform, home: Option<&Path>, path_env: Option<&str>) -> HarnessInventory {
    let home = home_dir(platform, home);
    let mut inventory = HarnessInventory::default();
    for def in all_definitions() {
        if def.detection_only {
            if let Some(exe) = def
                .executable_names
                .iter()
                .find_map(|n| find_executable(n, path_env))
            {
                inventory.installations.push(HarnessInstallation {
                    id: Uuid::new_v4(),
                    harness_type: HarnessType::Custom(def.id.to_string()),
                    executable_path: Some(exe),
                    version: None,
                    config_path: None,
                    detected_at: Utc::now(),
                    last_scanned_at: None,
                    status: InstallationStatus::Detected,
                });
            }
            continue;
        }
        let exe = def
            .executable_names
            .iter()
            .find_map(|n| find_executable(n, path_env));
        let config = resolve_config_path(&def, &home, platform).map(|c| c.display().to_string());
        let installation = match (exe, config) {
            (Some(executable_path), config_path) => {
                let version = detect_version(&executable_path, version_args_for(&def));
                HarnessInstallation {
                    id: Uuid::new_v4(),
                    harness_type: HarnessType::parse_str(def.id),
                    executable_path: Some(executable_path),
                    version,
                    config_path,
                    detected_at: Utc::now(),
                    last_scanned_at: Some(Utc::now()),
                    status: InstallationStatus::Installed,
                }
            }
            (None, Some(config_path)) => HarnessInstallation {
                id: Uuid::new_v4(),
                harness_type: HarnessType::parse_str(def.id),
                executable_path: None,
                version: None,
                config_path: Some(config_path),
                detected_at: Utc::now(),
                last_scanned_at: Some(Utc::now()),
                status: InstallationStatus::ConfigMissing,
            },
            (None, None) => continue,
        };
        inventory.installations.push(installation);
    }
    inventory
}
