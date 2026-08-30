//! Sync transaction + config snapshot repositories (audit + rollback).

use chm_core::domain::history::{
    ConfigSnapshot, SyncTransaction, TransactionStatus, TransactionType,
};
use chm_core::parse_ts;
use chrono::Utc;
use sqlx::{Pool, Sqlite};
use std::collections::HashMap;
use uuid::Uuid;

use crate::DbError;

pub async fn begin_transaction<'e>(
    pool: impl sqlx::Executor<'e, Database = Sqlite> + 'e,
    tx_type: TransactionType,
    plan: serde_json::Value,
) -> Result<SyncTransaction, DbError> {
    let tx = SyncTransaction {
        id: Uuid::new_v4(),
        transaction_type: tx_type,
        started_at: Utc::now(),
        completed_at: None,
        status: TransactionStatus::Running,
        summary: None,
        plan,
        error: None,
    };
    sqlx::query(
        "INSERT INTO sync_transactions (id, transaction_type, started_at, status, plan_json)
         VALUES (?, ?, ?, 'running', ?)",
    )
    .bind(tx.id.to_string())
    .bind(tx.transaction_type.as_str())
    .bind(tx.started_at.to_rfc3339())
    .bind(serde_json::to_string(&tx.plan)?)
    .execute(pool)
    .await?;
    Ok(tx)
}

pub async fn finish_transaction<'e>(
    pool: impl sqlx::Executor<'e, Database = Sqlite> + 'e,
    id: Uuid,
    status: TransactionStatus,
    summary: Option<String>,
    error: Option<String>,
) -> Result<(), DbError> {
    let res = sqlx::query(
        "UPDATE sync_transactions SET status = ?, summary = ?, error_json = ?, completed_at = ?
         WHERE id = ?",
    )
    .bind(status.as_str())
    .bind(&summary)
    .bind(&error)
    .bind(Utc::now().to_rfc3339())
    .bind(id.to_string())
    .execute(pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(DbError::NotFound(format!("transaction {id}")));
    }
    Ok(())
}

pub async fn add_snapshot<'e>(
    pool: impl sqlx::Executor<'e, Database = Sqlite> + 'e,
    s: &ConfigSnapshot,
) -> Result<(), DbError> {
    sqlx::query(
        "INSERT INTO config_snapshots
           (id, transaction_id, harness_installation_id, path, before_content, after_content,
            before_hash, after_hash)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(s.id.to_string())
    .bind(s.transaction_id.to_string())
    .bind(s.harness_installation_id.to_string())
    .bind(&s.path)
    .bind(&s.before_content)
    .bind(&s.after_content)
    .bind(&s.before_hash)
    .bind(&s.after_hash)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_transactions(pool: &Pool<Sqlite>) -> Result<Vec<SyncTransaction>, DbError> {
    list_transactions_with_limit(pool, None).await
}

/// List the newest transactions, applying the limit in SQLite before rows are
/// materialized. A negative SQLite LIMIT means "all rows", so the unbounded
/// API above can share the same row mapping without a second query shape.
pub async fn list_transactions_limited(
    pool: &Pool<Sqlite>,
    limit: u32,
) -> Result<Vec<SyncTransaction>, DbError> {
    list_transactions_with_limit(pool, Some(limit)).await
}

async fn list_transactions_with_limit(
    pool: &Pool<Sqlite>,
    limit: Option<u32>,
) -> Result<Vec<SyncTransaction>, DbError> {
    let rows = sqlx::query_as::<_, (String, String, String, Option<String>, String, Option<String>, Option<String>, String)>(
        "SELECT id, transaction_type, started_at, completed_at, status, summary, error_json, plan_json
         FROM sync_transactions ORDER BY started_at DESC LIMIT ?",
    )
    .bind(limit.map(i64::from).unwrap_or(-1))
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(
            |(id, tx_type, started, completed, status, summary, error, plan)| {
                Ok(SyncTransaction {
                    id: Uuid::parse_str(&id).map_err(|_| DbError::NotFound(id))?,
                    transaction_type: TransactionType::parse_str(&tx_type),
                    started_at: parse_ts(&started)?,
                    completed_at: completed.and_then(|t| parse_ts(&t).ok()),
                    status: TransactionStatus::parse_str(&status),
                    summary,
                    plan: serde_json::from_str(&plan).unwrap_or_default(),
                    error,
                })
            },
        )
        .collect()
}

pub async fn list_snapshots<'e>(
    pool: impl sqlx::Executor<'e, Database = Sqlite> + 'e,
    transaction_id: Uuid,
) -> Result<Vec<ConfigSnapshot>, DbError> {
    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        ),
    >(
        "SELECT id, transaction_id, harness_installation_id, path, before_content, after_content,
                before_hash, after_hash
         FROM config_snapshots WHERE transaction_id = ?",
    )
    .bind(transaction_id.to_string())
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(
            |(id, tid, hid, path, before, after, before_hash, after_hash)| {
                Ok(ConfigSnapshot {
                    id: Uuid::parse_str(&id).map_err(|_| DbError::NotFound(id))?,
                    transaction_id: Uuid::parse_str(&tid).map_err(|_| DbError::NotFound(tid))?,
                    harness_installation_id: Uuid::parse_str(&hid)
                        .map_err(|_| DbError::NotFound(hid))?,
                    path,
                    before_content: before,
                    after_content: after,
                    before_hash,
                    after_hash,
                })
            },
        )
        .collect()
}

/// Load snapshots for a bounded set of transactions in one query.
///
/// History screens commonly display the newest N transactions and previously
/// issued one query per transaction for its snapshots. Keeping the grouping in
/// this repository avoids that N+1 round-trip pattern while retaining the
/// single-transaction helper used by rollback.
pub async fn list_snapshots_for_transactions(
    pool: &Pool<Sqlite>,
    transaction_ids: &[Uuid],
) -> Result<HashMap<Uuid, Vec<ConfigSnapshot>>, DbError> {
    if transaction_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let placeholders = (0..transaction_ids.len())
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(",");
    let query = format!(
        "SELECT id, transaction_id, harness_installation_id, path, before_content,
                after_content, before_hash, after_hash
         FROM config_snapshots WHERE transaction_id IN ({placeholders}) ORDER BY rowid"
    );
    let mut request = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        ),
    >(&query);
    for transaction_id in transaction_ids {
        request = request.bind(transaction_id.to_string());
    }
    let rows = request.fetch_all(pool).await?;
    let mut grouped = HashMap::new();
    for (id, tid, hid, path, before, after, before_hash, after_hash) in rows {
        let snapshot = ConfigSnapshot {
            id: Uuid::parse_str(&id).map_err(|_| DbError::InvalidData(id))?,
            transaction_id: Uuid::parse_str(&tid).map_err(|_| DbError::InvalidData(tid))?,
            harness_installation_id: Uuid::parse_str(&hid)
                .map_err(|_| DbError::InvalidData(hid))?,
            path,
            before_content: before,
            after_content: after,
            before_hash,
            after_hash,
        };
        grouped
            .entry(snapshot.transaction_id)
            .or_insert_with(Vec::new)
            .push(snapshot);
    }
    Ok(grouped)
}

/// after_content of the newest snapshot for (installation, path) — last known state.
pub async fn latest_snapshot_content<'e>(
    pool: impl sqlx::Executor<'e, Database = Sqlite> + 'e,
    installation_id: Uuid,
    path: &str,
) -> Result<Option<String>, DbError> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT after_content FROM config_snapshots
         WHERE harness_installation_id = ? AND path = ? AND after_content IS NOT NULL
         ORDER BY rowid DESC LIMIT 1",
    )
    .bind(installation_id.to_string())
    .bind(path)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.0))
}
