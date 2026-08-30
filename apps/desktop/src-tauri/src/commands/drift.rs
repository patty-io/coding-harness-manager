//! Drift detection: compares the live config on disk with the last state
//! this app wrote (the newest config_snapshots row for the installation).
//!
//! The mirror-model promise is "what you see is what's on disk". When the
//! user edits a harness config by hand, the app must say so instead of
//! silently overwriting on the next apply.

use chm_core::domain::history::{ConfigSnapshot, TransactionStatus, TransactionType};
use chm_database::repos::history::{
    add_snapshot, begin_transaction, finish_transaction, latest_snapshot_content,
};
use serde::Serialize;
use tauri::State;
use uuid::Uuid;

use crate::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriftReport {
    pub installation_id: String,
    pub config_path: Option<String>,
    /// True once at least one apply has ever been recorded for this harness.
    pub ever_synced: bool,
    /// True when the file on disk differs from the last state we wrote.
    pub drifted: bool,
    pub current_content: Option<String>,
    pub last_written_content: Option<String>,
}

fn content_hash(s: &str) -> String {
    crate::drift::sha256_hex(s)
}

fn current_file_content(path: &str) -> Result<Option<String>, String> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(Some(content)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

/// Shared drift computation: true when the config on disk differs from the
/// newest snapshot this app wrote. Used by the per-harness command and by
/// the dashboard aggregate.
pub async fn installation_drifted(
    pool: &sqlx::Pool<sqlx::Sqlite>,
    inst: &chm_core::domain::harness::HarnessInstallation,
) -> Result<(bool, bool, Option<String>, Option<String>), String> {
    let Some(path) = inst.config_path.clone() else {
        return Ok((false, false, None, None));
    };
    let current = std::fs::read_to_string(&path).ok();
    let last_written = latest_snapshot_content(pool, inst.id, &path)
        .await
        .map_err(|e| e.to_string())?;
    let ever_synced = last_written.is_some();
    let drifted = match (&current, &last_written) {
        (Some(c), Some(w)) => content_hash(c) != content_hash(w),
        (None, Some(_)) => true, // the file we wrote was deleted
        _ => false,
    };
    Ok((ever_synced, drifted, current, last_written))
}

#[tauri::command]
pub async fn harness_drift_cmd(
    state: State<'_, AppState>,
    installation_id: String,
) -> Result<DriftReport, String> {
    let inst = crate::commands::find_installation(&state.pool, &installation_id).await?;
    let (ever_synced, drifted, current, last_written) =
        installation_drifted(&state.pool, &inst).await?;
    Ok(DriftReport {
        installation_id,
        config_path: inst.config_path,
        ever_synced,
        drifted,
        current_content: current,
        last_written_content: last_written,
    })
}

/// Accept the current on-disk content as the new known state without applying
/// anything. Used by the drift banner's "Accept local changes" action when
/// the user made the outside edit intentionally.
#[tauri::command]
pub async fn record_manual_snapshot_cmd(
    state: State<'_, AppState>,
    installation_id: String,
) -> Result<(), String> {
    let inst = crate::commands::find_installation(&state.pool, &installation_id).await?;
    let path = inst
        .config_path
        .clone()
        .ok_or("harness has no config path")?;
    let current = current_file_content(&path)?;
    let tx = begin_transaction(
        &state.pool,
        TransactionType::Manual,
        serde_json::json!({ "reason": "accept local changes as new baseline" }),
    )
    .await
    .map_err(|e| e.to_string())?;
    let hash = current.as_deref().map(content_hash);
    let result = add_snapshot(
        &state.pool,
        &ConfigSnapshot {
            id: Uuid::new_v4(),
            transaction_id: tx.id,
            harness_installation_id: inst.id,
            path: path.clone(),
            before_content: current.clone(),
            after_content: current.clone(),
            before_hash: hash.clone(),
            after_hash: hash,
        },
    )
    .await;
    match result {
        Ok(_) => finish_transaction(
            &state.pool,
            tx.id,
            TransactionStatus::Succeeded,
            Some("accepted local changes as new baseline".into()),
            None,
        )
        .await
        .map_err(|e| e.to_string()),
        Err(e) => {
            let _ = finish_transaction(
                &state.pool,
                tx.id,
                TransactionStatus::Failed,
                None,
                Some(e.to_string()),
            )
            .await;
            Err(e.to_string())
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevertBaselineReport {
    pub path: String,
    pub transaction_id: String,
}

/// Replace the live harness config with the newest configuration this app
/// wrote. The current file is backed up and recorded as the snapshot's
/// `before_content`, so the revert itself remains undoable from History.
pub async fn revert_to_baseline_core(
    pool: &sqlx::Pool<sqlx::Sqlite>,
    installation_id: &str,
) -> Result<RevertBaselineReport, String> {
    let inst = crate::commands::find_installation(pool, installation_id).await?;
    let path = inst
        .config_path
        .clone()
        .ok_or("harness has no config path")?;
    let baseline = latest_snapshot_content(pool, inst.id, &path)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("no saved app baseline exists for this harness")?;
    let current = current_file_content(&path)?;

    if current.as_deref() == Some(baseline.as_str()) {
        return Err("config already matches the last app baseline".into());
    }

    let tx = begin_transaction(
        pool,
        TransactionType::Manual,
        serde_json::json!({
            "reason": "revert external changes to last app baseline",
            "installation_id": installation_id,
            "path": path,
        }),
    )
    .await
    .map_err(|e| e.to_string())?;

    let backup = match chm_filesystem::backup_file(std::path::Path::new(&path)) {
        Ok(backup) => backup,
        Err(error) => {
            let message = format!("backup failed before revert: {error}");
            let _ = finish_transaction(
                pool,
                tx.id,
                TransactionStatus::Failed,
                None,
                Some(message.clone()),
            )
            .await;
            return Err(message);
        }
    };

    let snapshot = ConfigSnapshot {
        id: Uuid::new_v4(),
        transaction_id: tx.id,
        harness_installation_id: inst.id,
        path: path.clone(),
        before_content: current.clone(),
        after_content: Some(baseline.clone()),
        before_hash: current.as_deref().map(content_hash),
        after_hash: Some(content_hash(&baseline)),
    };
    if let Err(error) = add_snapshot(pool, &snapshot).await {
        let message = format!("could not record revert snapshot: {error}");
        let _ = finish_transaction(
            pool,
            tx.id,
            TransactionStatus::Failed,
            None,
            Some(message.clone()),
        )
        .await;
        return Err(message);
    }

    if let Err(error) = chm_filesystem::atomic_write(std::path::Path::new(&path), &baseline) {
        let recovery = chm_filesystem::restore_backup(&backup, std::path::Path::new(&path));
        let detail = match recovery {
            Ok(()) => error.to_string(),
            Err(recovery_error) => format!(
                "{error}; recovery also failed: {recovery_error}"
            ),
        };
        let _ = finish_transaction(
            pool,
            tx.id,
            TransactionStatus::Failed,
            None,
            Some(detail.clone()),
        )
        .await;
        return Err(format!("revert failed: {detail}"));
    }

    finish_transaction(
        pool,
        tx.id,
        TransactionStatus::Succeeded,
        Some("reverted external changes to last app baseline".into()),
        None,
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(RevertBaselineReport {
        path,
        transaction_id: tx.id.to_string(),
    })
}

/// Restore the newest app-written config after the user explicitly confirms
/// the destructive action in the drift banner.
#[tauri::command]
pub async fn revert_to_baseline_cmd(
    state: State<'_, AppState>,
    installation_id: String,
) -> Result<RevertBaselineReport, String> {
    revert_to_baseline_core(&state.pool, &installation_id).await
}
