//! Tauri backend: commands + app state.

pub mod commands;

use chm_database::connect;
use chm_secrets::{KeychainStore, SecretStore};
use sqlx::{Pool, Sqlite};
use tauri::Manager;

pub struct AppState {
    pub pool: Pool<Sqlite>,
    pub secrets: Box<dyn SecretStore>,
    pub http: reqwest::Client,
}

fn db_path() -> String {
    let dir = std::env::var_os("CHM_DATA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME").unwrap_or_default();
            std::path::PathBuf::from(home).join(".coding-harness-manager")
        });
    dir.join("chm.sqlite").display().to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let pool = tauri::async_runtime::block_on(connect(&db_path()))
                .expect("database connect");
            #[cfg(target_os = "macos")]
            let secrets: Box<dyn SecretStore> =
                Box::new(KeychainStore::new("coding-harness-manager"));
            #[cfg(not(target_os = "macos"))]
            let secrets: Box<dyn SecretStore> = Box::new(chm_secrets::EnvStore);
            app.manage(AppState {
                pool,
                secrets,
                http: reqwest::Client::new(),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::scan::scan_harnesses,
            commands::scan::list_installations_cmd,
            commands::import::read_harness_state,
            commands::import::import_harness_state,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

pub fn pool<'a>(state: &'a tauri::State<'_, AppState>) -> &'a Pool<Sqlite> {
    &state.pool
}