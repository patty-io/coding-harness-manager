//! Tauri backend: commands + app state.

pub mod commands;
pub mod drift;
pub mod services;
pub mod skill_lib;

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
fn init_logging() {
    let log_dir = std::env::var_os("CHM_DATA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME").unwrap_or_default();
            std::path::PathBuf::from(home).join(".coding-harness-manager")
        })
        .join("logs");
    let _ = std::fs::create_dir_all(&log_dir);
    let file_appender = tracing_appender::rolling::daily(&log_dir, "chm.log");
    let (writer, guard) = tracing_appender::non_blocking(file_appender);
    // the guard must outlive the subscriber — intentionally leaked
    std::mem::forget(guard);
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_writer(move || writer.clone())
        .with_env_filter(filter)
        .init();
    // panics land in the log too, so a crashed backend leaves evidence
    std::panic::set_hook(Box::new(|info| {
        tracing::error!("PANIC: {info}");
        let default = std::panic::take_hook();
        default(info);
    }));
}

pub fn run() {
    init_logging();
    tracing::info!("starting Coding Harness Manager");
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
            commands::import::read_harness_raw_config,
            commands::drift::harness_drift_cmd,
            commands::drift::record_manual_snapshot_cmd,
            commands::detect::detect_mcp_cmd,
            commands::detect::detect_skills_cmd,
            commands::harness_detail::harness_models_view_cmd,
            commands::harness_detail::adopt_harness_model_cmd,
            commands::harness_detail::list_endpoint_options_cmd,
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
            commands::provider_actions::discover_provider_models,
            commands::provider_actions::add_discovered_to_my_models_cmd,
            commands::provider_actions::list_provider_catalog_cmd,
            commands::provider_actions::list_catalog_models_cmd,
            commands::provider_actions::provider_summary,
            commands::models::list_routes_cmd,
            commands::models::update_route_cmd,
            commands::models::delete_route_cmd,
            commands::models::create_route_cmd,
            commands::models::list_catalog_all,
            commands::models::add_catalog_batch,
            commands::models::enrich_route_cmd,
            commands::models::resolve_enrichment_cmd,
            commands::models::set_user_override_cmd,
            commands::sync::sync_preview,
            commands::sync::sync_apply,
            commands::mcp::create_mcp_cmd,
            commands::mcp::list_mcp_cmd,
            commands::mcp::delete_mcp_cmd,
            commands::mcp::mcp_detail_cmd,
            commands::mcp::bind_mcp_cmd,
            commands::mcp::run_mcp_diagnostics,
            commands::skills::list_skills_cmd,
            commands::skills::scan_skills_dir_cmd,
            commands::skills::import_skills_cmd,
            commands::skills::adopt_canonical_dir,
            commands::skills::bind_skill_cmd,
            commands::skills::unbind_skill_cmd,
            commands::profiles::list_profiles_cmd,
            commands::profiles::create_profile_cmd,
            commands::profiles::delete_profile_cmd,
            commands::profiles::launch_profile_cmd,
            commands::sets::list_sets_cmd,
            commands::sets::create_set_cmd,
            commands::sets::add_set_item_cmd,
            commands::sets::remove_set_item_cmd,
            commands::sets::apply_set_preview_cmd,
            commands::sets::apply_set_cmd,
            commands::history::list_history_cmd,
            commands::history::rollback_transaction_cmd,
            commands::history::purge_old_snapshots_cmd,
            commands::doctor::run_doctor_cmd,
            commands::doctor::export_diagnostics_cmd,
            commands::logging::frontend_log_cmd,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

pub fn pool<'a>(state: &'a tauri::State<'_, AppState>) -> &'a Pool<Sqlite> {
    &state.pool
}
