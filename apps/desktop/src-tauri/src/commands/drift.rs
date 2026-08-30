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

/// Re-baseline: record the current on-disk content as the new known state
/// without applying anything. Used by the drift banner's "Mark as reviewed"
/// action when the user made the outside edit intentionally.
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
    let current = std::fs::read_to_string(&path).ok();
    let tx = begin_transaction(
        &state.pool,
        TransactionType::Manual,
        serde_json::json!({ "reason": "re-baseline after external edit" }),
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
            Some("re-baselined from disk".into()),
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
