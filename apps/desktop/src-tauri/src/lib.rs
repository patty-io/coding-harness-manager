//! Tauri backend: commands + app state.

pub mod commands;
pub mod services;

use chm_database::connect;
use chm_secrets::SecretStore;
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
    std::fs::create_dir_all(&dir).expect("create data directory");
    dir.join("chm.sqlite").display().to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let pool =
                tauri::async_runtime::block_on(connect(&db_path())).expect("database connect");
            app.manage(AppState {
                pool,
                secrets: chm_secrets::default_store(),
                http: reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(15))
                    .build()
                    .expect("http client"),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::scan::scan_harnesses,
            commands::scan::list_installations_cmd,
            commands::import::read_harness_state,
            commands::import::import_harness_state,
            commands::dashboard::dashboard_stats,
            commands::providers::create_provider_cmd,
            commands::providers::list_providers_cmd,
            commands::providers::update_provider_cmd,
            commands::providers::delete_provider_cmd,
            commands::endpoints::list_endpoints_cmd,
            commands::endpoints::create_endpoint_cmd,
            commands::endpoints::save_api_key,
            commands::endpoints::env_var_set,
            commands::provider_actions::check_endpoint_health,
            commands::provider_actions::discover_endpoint_models,
            commands::provider_actions::list_catalog_models_cmd,
            commands::provider_actions::provider_summary,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

pub fn pool<'a>(state: &'a tauri::State<'_, AppState>) -> &'a Pool<Sqlite> {
    &state.pool
}
