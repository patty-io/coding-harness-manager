//! Harness installation + model binding repositories.

use chm_core::domain::harness::{
    HarnessInstallation, HarnessModelBinding, HarnessType, InstallationStatus,
};
use chrono::Utc;
use sqlx::{Pool, Sqlite};
use uuid::Uuid;

use crate::DbError;

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
    Ok(i.clone())
}

pub async fn list_installations(pool: &Pool<Sqlite>) -> Result<Vec<HarnessInstallation>, DbError> {
    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            String,
            Option<String>,
            String,
        ),
    >(
        "SELECT id, harness_type, executable_path, version, config_path, detected_at,
                last_scanned_at, status
         FROM harness_installations ORDER BY harness_type",
    )
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(
            |(id, htype, exe, version, config, detected, last_scanned, status)| {
                Ok(HarnessInstallation {
                    id: Uuid::parse_str(&id).map_err(|_| DbError::NotFound(id))?,
                    harness_type: HarnessType::parse_str(&htype),
                    executable_path: exe,
                    version,
                    config_path: config,
                    detected_at: parse_ts(&detected),
                    last_scanned_at: last_scanned.map(|t| parse_ts(&t)),
                    status: InstallationStatus::parse_str(&status),
                })
            },
        )
        .collect()
}

pub async fn create_model_binding(
    pool: &Pool<Sqlite>,
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

pub async fn list_model_bindings(
    pool: &Pool<Sqlite>,
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
                    created_at: parse_ts(&created),
                    updated_at: parse_ts(&updated),
                })
            },
        )
        .collect()
}

fn parse_ts(s: &str) -> chrono::DateTime<Utc> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}
