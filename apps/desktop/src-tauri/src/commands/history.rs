//! History, rollback, and purge commands (Phase 12).

use chm_core::domain::history::{ConfigSnapshot, TransactionStatus, TransactionType};
use chm_database::repos::history::{
    add_snapshot, begin_transaction, finish_transaction, list_snapshots,
    list_snapshots_for_transactions, list_transactions, list_transactions_limited,
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
    pub can_rollback: bool,
    pub rollback_reason: Option<String>,
}

fn content_hash(content: Option<&str>) -> Option<String> {
    content.map(crate::drift::sha256_hex)
}

fn current_content(path: &str) -> Result<Option<String>, String> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(Some(content)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

fn restore_content(path: &str, content: Option<&String>) -> Result<(), String> {
    match content {
        Some(value) => chm_filesystem::atomic_write(std::path::Path::new(path), value)
            .map_err(|e| e.to_string()),
        None => match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.to_string()),
        },
    }
}

fn rollback_eligibility(
    status: TransactionStatus,
    transaction_type: TransactionType,
    snapshot_count: usize,
) -> (bool, Option<String>) {
    if transaction_type == TransactionType::Rollback {
        return (
            false,
            Some("rollback transactions cannot be rolled back".into()),
        );
    }
    if status != TransactionStatus::Succeeded {
        return (
            false,
            Some("only successful transactions can be rolled back".into()),
        );
    }
    if snapshot_count == 0 {
        return (false, Some("transaction has no file snapshots".into()));
    }
    (true, None)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LegacyPiModel {
    provider: String,
    id: String,
    name: String,
    context_window: Option<i64>,
}

fn parse_pi_models(content: Option<&str>) -> Option<std::collections::BTreeMap<String, LegacyPiModel>> {
    let value = serde_json::from_str::<serde_json::Value>(content?).ok()?;
    let providers = value.get("providers")?.as_object()?;
    let mut models = std::collections::BTreeMap::new();
    for (provider, value) in providers {
        let Some(entries) = value.get("models").and_then(|value| value.as_array()) else {
            continue;
        };
        for model in entries {
            let Some(id) = model.get("id").and_then(|value| value.as_str()) else {
                continue;
            };
            let id = id.to_string();
            let name = model
                .get("name")
                .and_then(|value| value.as_str())
                .unwrap_or(&id)
                .to_string();
            let context_window = model
                .get("contextWindow")
                .or_else(|| model.get("context_window"))
                .and_then(|value| value.as_i64());
            let key = format!("{provider}\u{1f}{id}");
            models.insert(
                key,
                LegacyPiModel {
                    provider: provider.clone(),
                    id,
                    name,
                    context_window,
                },
            );
        }
    }
    Some(models)
}

fn compact_activity_value(value: &str) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = compact.chars();
    let short = chars.by_ref().take(120).collect::<String>();
    if chars.next().is_some() {
        format!("{short}…")
    } else {
        short
    }
}

fn history_harness_label(value: &str) -> String {
    let mut chars = value.replace('-', " ").chars().collect::<Vec<_>>();
    if let Some(first) = chars.first_mut() {
        first.make_ascii_uppercase();
    }
    chars.into_iter().collect()
}

fn legacy_model_label(model: &LegacyPiModel) -> String {
    let id = compact_activity_value(&model.id);
    let name = compact_activity_value(&model.name);
    if name.eq_ignore_ascii_case(&model.id) {
        id
    } else {
        format!("{id} ({name})")
    }
}

fn format_legacy_context(value: Option<i64>) -> String {
    value
        .map(|number| number.to_string())
        .unwrap_or_else(|| "unset".into())
}

fn parse_legacy_edit_summary(summary: &str) -> Option<(&str, usize, usize, usize)> {
    let rest = summary.strip_prefix("edited models on ")?;
    let (harness, counts) = rest.split_once(": ")?;
    let mut added = None;
    let mut updated = None;
    let mut removed = None;
    for token in counts.split_whitespace() {
        let (prefix, value) = token.split_at(1);
        let count = value.parse::<usize>().ok()?;
        match prefix {
            "+" => added = Some(count),
            "~" => updated = Some(count),
            "-" => removed = Some(count),
            _ => return None,
        }
    }
    Some((
        harness,
        added?,
        updated?,
        removed?,
    ))
}

fn describe_legacy_pi_changes(
    harness: &str,
    expected_added: usize,
    expected_updated: usize,
    expected_removed: usize,
    snapshots: &[ConfigSnapshot],
) -> Option<String> {
    let (before, after) = snapshots.iter().find_map(|snapshot| {
        Some((
            parse_pi_models(snapshot.before_content.as_deref())?,
            parse_pi_models(snapshot.after_content.as_deref())?,
        ))
    })?;
    let added = after
        .iter()
        .filter(|(key, _)| !before.contains_key(*key))
        .map(|(_, model)| model)
        .collect::<Vec<_>>();
    let removed = before
        .iter()
        .filter(|(key, _)| !after.contains_key(*key))
        .map(|(_, model)| model)
        .collect::<Vec<_>>();
    let updated = before
        .iter()
        .filter_map(|(key, old)| {
            let new = after.get(key)?;
            (old != new).then_some((old, new))
        })
        .collect::<Vec<_>>();
    if added.len() != expected_added
        || updated.len() != expected_updated
        || removed.len() != expected_removed
    {
        return None;
    }

    let harness = history_harness_label(harness);
    let mut details = Vec::new();
    for model in added {
        details.push(format!(
            "Added model {} via {}",
            legacy_model_label(model),
            compact_activity_value(&model.provider)
        ));
    }
    for (old, new) in updated {
        let mut changes = Vec::new();
        if old.name != new.name {
            changes.push(format!(
                "display name \"{}\" → \"{}\"",
                compact_activity_value(&old.name),
                compact_activity_value(&new.name)
            ));
        }
        if old.context_window != new.context_window {
            changes.push(format!(
                "context window {} → {}",
                format_legacy_context(old.context_window),
                format_legacy_context(new.context_window)
            ));
        }
        let suffix = if changes.is_empty() {
            String::new()
        } else {
            format!(": {}", changes.join(", "))
        };
        details.push(format!(
            "Updated model {} via {}{suffix}",
            legacy_model_label(new),
            compact_activity_value(&new.provider)
        ));
    }
    for model in removed {
        details.push(format!(
            "Deleted model {} via {}",
            legacy_model_label(model),
            compact_activity_value(&model.provider)
        ));
    }
    (!details.is_empty()).then(|| format!("{harness}: {}", details.join("; ")))
}

/// Upgrade the old counter-only edit summaries when the snapshots contain a
/// Pi model document. Other legacy summaries still get readable words rather
/// than unexplained `+0 ~0 -1` notation.
fn humanize_history_summary(summary: Option<&str>, snapshots: &[ConfigSnapshot]) -> Option<String> {
    let summary = summary?;
    let Some((harness, added, updated, removed)) = parse_legacy_edit_summary(summary) else {
        return Some(summary.to_string());
    };
    if let Some(details) = describe_legacy_pi_changes(
        harness,
        added,
        updated,
        removed,
        snapshots,
    ) {
        return Some(details);
    }
    Some(format!(
        "{}: {} model(s) added, {} updated, {} deleted",
        history_harness_label(harness),
        added,
        updated,
        removed
    ))
}

#[tauri::command]
pub async fn list_history_cmd(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> Result<Vec<HistoryEntry>, String> {
    let pool = &state.pool;
    let limit = limit.unwrap_or(100).max(1);
    let txs = list_transactions_limited(pool, limit)
        .await
        .map_err(|e| e.to_string())?;
    let transaction_ids: Vec<Uuid> = txs.iter().map(|tx| tx.id).collect();
    let snapshots_by_transaction = list_snapshots_for_transactions(pool, &transaction_ids)
        .await
        .map_err(|e| e.to_string())?;
    let mut entries = Vec::new();
    for tx in txs {
        let snaps = snapshots_by_transaction
            .get(&tx.id)
            .cloned()
            .unwrap_or_default();
        let (can_rollback, rollback_reason) =
            rollback_eligibility(tx.status, tx.transaction_type, snaps.len());
        let summary = humanize_history_summary(tx.summary.as_deref(), &snaps);
        entries.push(HistoryEntry {
            transaction_id: tx.id.to_string(),
            transaction_type: tx.transaction_type.as_str().to_string(),
            status: tx.status.as_str().to_string(),
            started_at: tx.started_at.to_rfc3339(),
            summary,
            snapshots: snaps
                .into_iter()
                .map(|s| SnapshotEntry {
                    path: s.path,
                    before: s.before_content,
                    after: s.after_content,
                })
                .collect(),
            can_rollback,
            rollback_reason,
        });
    }
    Ok(entries)
}

pub async fn rollback_transaction_core(
    pool: &Pool<Sqlite>,
    tx_id: &str,
) -> Result<RollbackReport, String> {
    let id = Uuid::parse_str(tx_id).map_err(|e| e.to_string())?;
    let tx = list_transactions(pool)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|tx| tx.id == id)
        .ok_or_else(|| format!("transaction {tx_id} not found"))?;
    let snaps = list_snapshots(pool, id).await.map_err(|e| e.to_string())?;
    let (eligible, reason) = rollback_eligibility(tx.status, tx.transaction_type, snaps.len());
    if !eligible {
        return Err(reason.unwrap_or_else(|| "transaction cannot be rolled back".into()));
    }

    // Never overwrite an intervening user edit. A rollback is valid only when
    // every file still matches the state written by the original transaction.
    let mut current = Vec::with_capacity(snaps.len());
    let mut conflicts = Vec::new();
    for snap in &snaps {
        let content = current_content(&snap.path)?;
        let actual_hash = content_hash(content.as_deref());
        let expected_hash = snap
            .after_hash
            .clone()
            .or_else(|| content_hash(snap.after_content.as_deref()));
        if actual_hash != expected_hash {
            conflicts.push(format!(
                "{} (expected {}, found {})",
                snap.path,
                expected_hash.as_deref().unwrap_or("missing"),
                actual_hash.as_deref().unwrap_or("missing")
            ));
        }
        current.push(content);
    }
    if !conflicts.is_empty() {
        return Err(format!(
            "rollback blocked: files changed since the transaction: {}",
            conflicts.join(", ")
        ));
    }

    // Record the current state before restoring the old state. This makes the
    // rollback itself a first-class reversible transaction while preserving a
    // link to the transaction it reverses.
    let rollback_tx = begin_transaction(
        pool,
        TransactionType::Rollback,
        serde_json::json!({
            "rolled_back": id.to_string(),
            "files": snaps.iter().map(|s| s.path.clone()).collect::<Vec<_>>()
        }),
    )
    .await
    .map_err(|e| e.to_string())?;
    for (snap, before_restore) in snaps.iter().zip(current.iter()) {
        if let Err(error) = add_snapshot(
            pool,
            &chm_core::domain::history::ConfigSnapshot {
                id: Uuid::new_v4(),
                transaction_id: rollback_tx.id,
                harness_installation_id: snap.harness_installation_id,
                path: snap.path.clone(),
                before_content: before_restore.clone(),
                after_content: snap.before_content.clone(),
                before_hash: content_hash(before_restore.as_deref()),
                after_hash: content_hash(snap.before_content.as_deref()),
            },
        )
        .await
        {
            let message = format!("could not record rollback snapshot: {error}");
            let _ = finish_transaction(
                pool,
                rollback_tx.id,
                TransactionStatus::Failed,
                None,
                Some(message.clone()),
            )
            .await;
            return Err(message);
        }
    }
    let mut files_restored = Vec::new();
    for (index, snap) in snaps.iter().enumerate() {
        if let Err(error) = restore_content(&snap.path, snap.before_content.as_ref()) {
            // A failure halfway through a multi-file restore must not leave a
            // mix of old and new config. Put every touched path back to the
            // exact state observed before rollback, then close the audit row
            // as Failed so it is never stuck in Running.
            let mut recovery_errors = Vec::new();
            for (recovery_snap, before_restore) in snaps.iter().zip(current.iter()).take(index + 1)
            {
                if let Err(recovery_error) =
                    restore_content(&recovery_snap.path, before_restore.as_ref())
                {
                    recovery_errors.push(format!("{}: {recovery_error}", recovery_snap.path));
                }
            }
            let detail = if recovery_errors.is_empty() {
                error.to_string()
            } else {
                format!(
                    "{error}; recovery also failed: {}",
                    recovery_errors.join("; ")
                )
            };
            let _ = finish_transaction(
                pool,
                rollback_tx.id,
                TransactionStatus::Failed,
                None,
                Some(detail.clone()),
            )
            .await;
            return Err(format!("rollback failed: {detail}"));
        }
        files_restored.push(snap.path.clone());
    }
    finish_transaction(
        pool,
        rollback_tx.id,
        TransactionStatus::Succeeded,
        Some(format!(
            "rolled back {} (reverses {})",
            files_restored.len(),
            id
        )),
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
pub async fn purge_old_snapshots_core(pool: &Pool<Sqlite>) -> Result<usize, String> {
    let cutoff = (chrono::Utc::now() - chrono::Duration::days(90)).to_rfc3339();
    // count first
    let count_row: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM config_snapshots WHERE transaction_id IN (SELECT id FROM sync_transactions WHERE started_at < ?)")
            .bind(cutoff.clone())
            .fetch_one(pool)
            .await
            .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM config_snapshots WHERE transaction_id IN (SELECT id FROM sync_transactions WHERE started_at < ?)")
        .bind(cutoff.clone())
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM sync_transactions WHERE started_at < ?")
        .bind(cutoff)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(count_row.0 as usize)
}

#[tauri::command]
pub async fn purge_old_snapshots_cmd(state: State<'_, AppState>) -> Result<usize, String> {
    let audit = begin_transaction(
        &state.pool,
        TransactionType::Manual,
        serde_json::json!({"action":"purge_old_snapshots", "retention_days":90}),
    )
    .await
    .map_err(|e| e.to_string())?;
    match purge_old_snapshots_core(&state.pool).await {
        Ok(count) => {
            finish_transaction(
                &state.pool,
                audit.id,
                TransactionStatus::Succeeded,
                Some(format!("purged {count} snapshot(s) older than 90 days")),
                None,
            )
            .await
            .map_err(|e| e.to_string())?;
            Ok(count)
        }
        Err(error) => {
            let _ = finish_transaction(
                &state.pool,
                audit.id,
                TransactionStatus::Failed,
                None,
                Some(error.clone()),
            )
            .await;
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_successful_snapshot_transactions_are_rollback_eligible() {
        assert_eq!(
            rollback_eligibility(TransactionStatus::Succeeded, TransactionType::Sync, 1),
            (true, None)
        );
        assert!(!rollback_eligibility(TransactionStatus::Failed, TransactionType::Sync, 1).0);
        assert!(!rollback_eligibility(TransactionStatus::Running, TransactionType::Sync, 1).0);
        assert!(!rollback_eligibility(TransactionStatus::Succeeded, TransactionType::Sync, 0).0);
        assert!(
            !rollback_eligibility(TransactionStatus::Succeeded, TransactionType::Rollback, 1).0
        );
    }

    #[test]
    fn content_hash_distinguishes_missing_and_empty_files() {
        assert_ne!(content_hash(None), content_hash(Some("")));
        assert_eq!(content_hash(Some("same")), content_hash(Some("same")));
    }

    #[test]
    fn legacy_pi_edit_summary_is_upgraded_from_snapshots() {
        let before = r#"{
            "providers": {
                "Yolo-Auto": {
                    "models": [
                        {"id": "qwen3.8-27b", "name": "Qwen 3.8 27B"},
                        {"id": "keep", "name": "Keep"}
                    ]
                }
            }
        }"#;
        let after = r#"{
            "providers": {
                "Yolo-Auto": {
                    "models": [
                        {"id": "keep", "name": "Keep"}
                    ]
                }
            }
        }"#;
        let snapshot = ConfigSnapshot {
            id: Uuid::new_v4(),
            transaction_id: Uuid::new_v4(),
            harness_installation_id: Uuid::new_v4(),
            path: "/tmp/models.json".into(),
            before_content: Some(before.into()),
            after_content: Some(after.into()),
            before_hash: None,
            after_hash: None,
        };
        let summary = humanize_history_summary(
            Some("edited models on pi: +0 ~0 -1"),
            &[snapshot],
        )
        .unwrap();
        assert_eq!(
            summary,
            "Pi: Deleted model qwen3.8-27b (Qwen 3.8 27B) via Yolo-Auto"
        );
    }

    #[test]
    fn legacy_counter_summary_falls_back_to_words_without_model_snapshot() {
        let summary = humanize_history_summary(Some("edited models on pi: +1 ~2 -0"), &[])
            .unwrap();
        assert_eq!(summary, "Pi: 1 model(s) added, 2 updated, 0 deleted");
    }
}
