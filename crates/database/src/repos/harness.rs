//! Harness installation + model binding repositories.

use chm_core::domain::harness::{
    HarnessInstallation, HarnessModelBinding, HarnessType, InstallationStatus,
};
use chm_core::parse_ts;
use sqlx::{Pool, Sqlite};
use uuid::Uuid;

use crate::DbError;

type InstallationRow = (
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
    Option<String>,
    String,
);

fn parse_installation_row(
    (id, htype, executable_path, version, config_path, detected_at, last_scanned_at, status):
        InstallationRow,
) -> Result<HarnessInstallation, DbError> {
    Ok(HarnessInstallation {
        id: Uuid::parse_str(&id).map_err(|_| DbError::InvalidData(id))?,
        harness_type: HarnessType::parse_str(&htype),
        executable_path,
        version,
        config_path,
        detected_at: parse_ts(&detected_at)?,
        last_scanned_at: last_scanned_at.and_then(|t| parse_ts(&t).ok()),
        status: InstallationStatus::parse_str(&status),
    })
}

pub async fn upsert_installation(
    pool: &Pool<Sqlite>,
    i: &HarnessInstallation,
) -> Result<HarnessInstallation, DbError> {
    sqlx::query(
        "INSERT INTO harness_installations
           (id, harness_type, executable_path, version, config_path, detected_at,
            last_scanned_at, status)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (harness_type) DO UPDATE SET
           executable_path = excluded.executable_path,
           version = excluded.version,
           config_path = excluded.config_path,
           last_scanned_at = excluded.last_scanned_at,
           status = excluded.status",
    )
    .bind(i.id.to_string())
    .bind(i.harness_type.as_str())
    .bind(&i.executable_path)
    .bind(&i.version)
    .bind(&i.config_path)
    .bind(i.detected_at.to_rfc3339())
    .bind(i.last_scanned_at.map(|t| t.to_rfc3339()))
    .bind(i.status.as_str())
    .execute(pool)
    .await?;
    // The stored row owns the canonical id (stable across rescans) — return it,
    // not the caller-supplied fresh uuid.
    let row = sqlx::query_as::<_, InstallationRow>(
        "SELECT id, harness_type, executable_path, version, config_path, detected_at,
                last_scanned_at, status
         FROM harness_installations WHERE harness_type = ?",
    )
    .bind(i.harness_type.as_str())
    .fetch_one(pool)
    .await?;
    parse_installation_row(row)
}

pub async fn list_installations(pool: &Pool<Sqlite>) -> Result<Vec<HarnessInstallation>, DbError> {
    let rows = sqlx::query_as::<_, InstallationRow>(
        "SELECT id, harness_type, executable_path, version, config_path, detected_at,
                last_scanned_at, status
         FROM harness_installations ORDER BY harness_type",
    )
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(parse_installation_row).collect()
}

/// Load one installation without materializing every harness row. Commands
/// operating on a single harness use this path for a bounded database query.
pub async fn find_installation(
    pool: &Pool<Sqlite>,
    id: Uuid,
) -> Result<HarnessInstallation, DbError> {
    let row = sqlx::query_as::<_, InstallationRow>(
        "SELECT id, harness_type, executable_path, version, config_path, detected_at,
                last_scanned_at, status
         FROM harness_installations WHERE id = ?",
    )
    .bind(id.to_string())
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| DbError::NotFound(format!("installation {id}")))?;
    parse_installation_row(row)
}

pub async fn create_model_binding<'e>(
    pool: impl sqlx::Executor<'e, Database = Sqlite> + 'e,
    b: &HarnessModelBinding,
) -> Result<(), DbError> {
    sqlx::query(
        "INSERT INTO harness_model_bindings
           (id, harness_installation_id, model_route_id, native_id, native_config_json,
            managed, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(b.id.to_string())
    .bind(b.harness_installation_id.to_string())
    .bind(b.model_route_id.to_string())
    .bind(&b.native_id)
    .bind(serde_json::to_string(&b.native_config)?)
    .bind(b.managed as i64)
    .bind(b.created_at.to_rfc3339())
    .bind(b.updated_at.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

/// Record ownership of a native model without creating duplicate rows when a
/// sync is repeated. Bindings are keyed by installation, route, and native id
/// because a harness may expose the same remote id through multiple providers.
pub async fn upsert_model_binding(
    pool: &Pool<Sqlite>,
    b: &HarnessModelBinding,
) -> Result<(), DbError> {
    let existing = sqlx::query_scalar::<_, String>(
        "SELECT id FROM harness_model_bindings
         WHERE harness_installation_id = ? AND model_route_id = ? AND native_id = ?
         ORDER BY updated_at DESC LIMIT 1",
    )
    .bind(b.harness_installation_id.to_string())
    .bind(b.model_route_id.to_string())
    .bind(&b.native_id)
    .fetch_optional(pool)
    .await?;
    if let Some(id) = existing {
        sqlx::query(
            "UPDATE harness_model_bindings
             SET native_config_json = ?, managed = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(serde_json::to_string(&b.native_config)?)
        .bind(b.managed as i64)
        .bind(b.updated_at.to_rfc3339())
        .bind(id)
        .execute(pool)
        .await?;
        Ok(())
    } else {
        create_model_binding(pool, b).await
    }
}

pub async fn list_model_bindings<'e>(
    pool: impl sqlx::Executor<'e, Database = Sqlite> + 'e,
    installation_id: Uuid,
) -> Result<Vec<HarnessModelBinding>, DbError> {
    let rows = sqlx::query_as::<_, (String, String, String, String, String, i64, String, String)>(
        "SELECT id, harness_installation_id, model_route_id, native_id, native_config_json,
                managed, created_at, updated_at
         FROM harness_model_bindings WHERE harness_installation_id = ?",
    )
    .bind(installation_id.to_string())
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(
            |(id, hid, rid, native_id, native_config, managed, created, updated)| {
                Ok(HarnessModelBinding {
                    id: Uuid::parse_str(&id).map_err(|_| DbError::NotFound(id))?,
                    harness_installation_id: Uuid::parse_str(&hid)
                        .map_err(|_| DbError::NotFound(hid))?,
                    model_route_id: Uuid::parse_str(&rid).map_err(|_| DbError::NotFound(rid))?,
                    native_id,
                    native_config: serde_json::from_str(&native_config).unwrap_or_default(),
                    managed: managed == 1,
                    created_at: parse_ts(&created)?,
                    updated_at: parse_ts(&updated)?,
                })
            },
        )
        .collect()
}
