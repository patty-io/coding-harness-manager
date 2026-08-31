pub mod backup;
pub mod dashboard;
pub mod detect;
pub mod doctor;
pub mod drift;
pub mod endpoints;
pub mod harness_detail;
pub mod history;
pub mod import;
pub mod launcher;
pub mod logging;
pub mod mcp;
pub mod models;
pub mod profiles;
pub mod provider_actions;
pub mod providers;
pub mod scan;
pub mod settings;
pub mod sets;
pub mod skills;
pub mod sync;

use chm_core::domain::harness::HarnessInstallation;
use chm_database::repos::harness::find_installation as find_installation_row;
use sqlx::{Pool, Sqlite};

/// Load one persisted harness installation by its stable id.
///
/// Commands that operate on a single harness should share this lookup rather
/// than each loading the complete installation list and reimplementing the
/// same UUID/error handling.
pub async fn find_installation(
    pool: &Pool<Sqlite>,
    installation_id: &str,
) -> Result<HarnessInstallation, String> {
    let id = uuid::Uuid::parse_str(installation_id).map_err(|e| e.to_string())?;
    find_installation_row(pool, id)
        .await
        .map_err(|error| match error {
            chm_database::DbError::NotFound(_) => {
                format!("installation {installation_id} not found")
            }
            other => other.to_string(),
        })
}
