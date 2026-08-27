//! Orchestrates full-machine detection into a normalized inventory.

use std::path::Path;

use chm_core::domain::harness::HarnessInstallation;

use crate::adapter::helpers::detect_one;
use crate::definition::Platform;
use crate::definition::all_definitions;

use super::paths::home_dir;

#[derive(Debug, Default)]
pub struct HarnessInventory {
    pub installations: Vec<HarnessInstallation>,
}

pub fn scan(platform: Platform, home: Option<&Path>, path_env: Option<&str>) -> HarnessInventory {
    let home = home_dir(platform, home);
    let mut inventory = HarnessInventory::default();
    for def in all_definitions() {
        if let Some(installation) = detect_one(&def, &home, platform, path_env) {
            inventory.installations.push(installation);
        }
    }
    inventory
}
