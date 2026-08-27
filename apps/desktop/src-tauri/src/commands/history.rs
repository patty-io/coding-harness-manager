//! History, rollback, and purge commands (Phase 12).

use chm_core::domain::history::{TransactionStatus, TransactionType};
use chm_database::repos::harness::list_installations;
use chm_database::repos::history::{
    add_snapshot, begin_transaction, finish_transaction, list_snapshots, list_transactions,
};
use serde::Serialize;
use sqlx::{Pool, Sqlite};
use tauri::State;
use uuid::Uuid;

use crate::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotEntry {
    pub path: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub transaction_id: String,
    pub transaction_type: String,
    pub status: String,
    pub started_at: String,
    pub summary: Option<String>,
    pub snapshots: Vec<SnapshotEntry>,
}

#[tauri::command]
pub async fn list_history_cmd(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> Result<Vec<HistoryEntry>, String> {
    let pool = &state.pool;
    let txs = list_transactions(pool).await.map_err(|e| e.to_string())?;
    let mut entries = Vec::new();
    for tx in txs.into_iter().take(limit.unwrap_or(100).max(1) as usize) {
        let snaps = list_snapshots(pool, tx.id).await.map_err(|e| e.to_string())?;
        entries.push(HistoryEntry {
            transaction_id: tx.id.to_string(),
            transaction_type: tx.transaction_type.as_str().to_string(),
            status: tx.status.as_str().to_string(),
            started_at: tx.started_at.to_rfc3339(),
            summary: tx.summary.clone(),
            snapshots: snaps
                .into_iter()
                .map(|s| SnapshotEntry {
                    path: s.path,
                    before: s.before_content,
                    after: s.after_content,
                })
                .collect(),
        });
    }
    Ok(entries)
}

pub async fn rollback_transaction_core(
    pool: &Pool<Sqlite>,
    tx_id: &str,
) -> Result<RollbackReport, String> {
    let id = Uuid::parse_str(tx_id).map_err(|e| e.to_string())?;
    let snaps = list_snapshots(pool, id).await.map_err(|e| e.to_string())?;
    let mut files_restored = Vec::new();
    for snap in snaps.iter().rev() {
        match &snap.before_content {
            Some(before) => {
                chm_filesystem::atomic_write(std::path::Path::new(&snap.path), before)
                    .map_err(|e| e.to_string())?;
                files_restored.push(snap.path.clone());
            }
            None => {
                // no backup = file was CREATED by this transaction — remove it
                let path = std::path::Path::new(&snap.path);
                if path.exists() {
                    std::fs::remove_file(path).map_err(|e| e.to_string())?;
                    files_restored.push(snap.path.clone());
                }
            }
        }
    }
    let rollback_tx = begin_transaction(
        pool,
        TransactionType::Rollback,
        serde_json::json!({ "rolled_back": id.to_string(), "files": files_restored }),
    )
    .await
    .map_err(|e| e.to_string())?;
    finish_transaction(
        pool,
        rollback_tx.id,
        TransactionStatus::Succeeded,
        Some(format!("rolled back {}", files_restored.len())),
        None,
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok(RollbackReport {
        files_restored,
        new_transaction_id: rollback_tx.id.to_string(),
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RollbackReport {
    pub files_restored: Vec<String>,
    pub new_transaction_id: String,
}

#[tauri::command]
pub async fn rollback_transaction_cmd(
    state: State<'_, AppState>,
    transaction_id: String,
) -> Result<RollbackReport, String> {
    rollback_transaction_core(&state.pool, &transaction_id).await
}

#[tauri::command]
pub async fn purge_old_snapshots_cmd(state: State<'_, AppState>) -> Result<usize, String> {
    let cutoff = (chrono::Utc::now() - chrono::Duration::days(90)).to_rfc3339();
    // count first
    let count_row: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM config_snapshots WHERE transaction_id IN (SELECT id FROM sync_transactions WHERE started_at < ?)")
            .bind(cutoff.clone())
            .fetch_one(&state.pool)
            .await
            .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM config_snapshots WHERE transaction_id IN (SELECT id FROM sync_transactions WHERE started_at < ?)")
        .bind(cutoff.clone())
        .execute(&state.pool)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM sync_transactions WHERE started_at < ?")
        .bind(cutoff)
        .execute(&state.pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(count_row.0 as usize)
}