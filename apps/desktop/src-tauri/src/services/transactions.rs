//! Shared transaction recovery mechanics for native harness file writes.
//!
//! Commands remain responsible for deciding when a write is a failure and
//! what the user-facing message should say. This service owns the repeated
//! recovery sequence: ask the adapter to roll back, restore every filesystem
//! backup, and close the audit transaction with the combined result.

use chm_core::domain::harness::HarnessInstallation;
use chm_core::domain::history::TransactionStatus;
use chm_database::repos::history::finish_transaction;
use chm_filesystem::restore_backup;
use chm_harness_sdk::adapter::types::{HarnessAdapter, NativePlan};
use sqlx::{Pool, Sqlite};
use std::path::Path;
use uuid::Uuid;

/// Restore a failed native write and always close its audit transaction.
///
/// The backup tuple is `(target file, backup file)`. Keeping this operation in
/// one place prevents command-specific rollback paths from drifting (notably,
/// swapping the source and destination arguments to `restore_backup`).
pub async fn rollback_native_transaction(
    pool: &Pool<Sqlite>,
    transaction_id: Uuid,
    adapter: &dyn HarnessAdapter,
    installation: &HarnessInstallation,
    native_plan: &NativePlan,
    backups: &[(String, std::path::PathBuf)],
    errors: &[String],
) -> Result<(), String> {
    let mut recovery_errors = Vec::new();
    if let Err(error) = adapter.rollback(installation, native_plan) {
        recovery_errors.push(format!("adapter rollback: {error}"));
    }
    for (target, backup) in backups {
        if let Err(error) = restore_backup(backup, Path::new(target)) {
            recovery_errors.push(format!("{target}: {error}"));
        }
    }

    let detail = if recovery_errors.is_empty() {
        errors.join("; ")
    } else {
        format!(
            "{}; recovery also failed: {}",
            errors.join("; "),
            recovery_errors.join("; ")
        )
    };
    finish_transaction(
        pool,
        transaction_id,
        TransactionStatus::Failed,
        None,
        Some(detail.clone()),
    )
    .await
    .map_err(|error| format!("{detail}; could not close transaction: {error}"))?;

    if recovery_errors.is_empty() {
        Ok(())
    } else {
        Err(detail)
    }
}
