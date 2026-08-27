//! Scan + inventory commands.

use chm_core::domain::harness::HarnessInstallation;
use chm_database::repos::harness::{list_installations, upsert_installation};
use chm_harness_sdk::definition::Platform;
use chm_harness_sdk::detect::scan::scan;
use sqlx::{Pool, Sqlite};
use tauri::State;

use crate::AppState;

pub async fn scan_and_persist(pool: &Pool<Sqlite>) -> Result<Vec<HarnessInstallation>, String> {
    #[cfg(target_os = "macos")]
    let platform = Platform::MacOs;
    #[cfg(target_os = "windows")]
    let platform = Platform::Windows;
    #[cfg(all(unix, not(target_os = "macos")))]
    let platform = Platform::Linux;

    let inventory = scan(platform, None, None);
    for inst in &inventory.installations {
        upsert_installation(pool, inst)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(inventory.installations)
}

#[tauri::command]
pub async fn scan_harnesses(
    state: State<'_, AppState>,
) -> Result<Vec<HarnessInstallation>, String> {
    scan_and_persist(&state.pool).await
}

/// PATH lookup used by MCP diagnostics.
pub fn command_on_path(name: &str) -> bool {
    if name.contains('/') {
        return std::path::Path::new(name).is_file();
    }
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|dir| dir.join(name).is_file()))
        .unwrap_or(false)
}

#[tauri::command]
pub async fn list_installations_cmd(
    state: State<'_, AppState>,
) -> Result<Vec<HarnessInstallation>, String> {
    list_installations(&state.pool)
        .await
        .map_err(|e| e.to_string())
}
