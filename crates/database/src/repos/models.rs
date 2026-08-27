//! Identity, catalog, and route repositories.

use chm_core::domain::models::*;
use chrono::Utc;
use sqlx::{Pool, Sqlite};
use uuid::Uuid;

use crate::DbError;

pub async fn create_identity(
    pool: &Pool<Sqlite>,
    i: &ModelIdentity,
) -> Result<ModelIdentity, DbError> {
    sqlx::query(
        "INSERT INTO model_identities
           (id, canonical_id, display_name, family, models_dev_id, metadata_json, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(i.id.to_string())
    .bind(&i.canonical_id)
    .bind(&i.display_name)
    .bind(&i.family)
    .bind(&i.models_dev_id)
    .bind(serde_json::to_string(&i.metadata)?)
    .bind(i.created_at.to_rfc3339())
    .bind(i.updated_at.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(i.clone())
}

pub async fn upsert_catalog_model(
    pool: &Pool<Sqlite>,
    m: &ProviderCatalogModel,
) -> Result<ProviderCatalogModel, DbError> {
    sqlx::query(
        "INSERT INTO provider_catalog_models
           (id, endpoint_id, remote_model_id, raw_metadata_json, canonical_model_id,
            match_confidence, first_seen_at, last_seen_at, missing_since, status)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (endpoint_id, remote_model_id) DO UPDATE SET
           raw_metadata_json = excluded.raw_metadata_json,
           canonical_model_id = excluded.canonical_model_id,
           match_confidence = excluded.match_confidence,
           last_seen_at = excluded.last_seen_at,
           missing_since = excluded.missing_since,
           status = excluded.status",
    )
    .bind(m.id.to_string())
    .bind(m.endpoint_id.to_string())
    .bind(&m.remote_model_id)
    .bind(serde_json::to_string(&m.raw_metadata)?)
    .bind(m.canonical_model_id.map(|id| id.to_string()))
    .bind(m.match_confidence.map(|c| c as i64))
    .bind(m.first_seen_at.to_rfc3339())
    .bind(m.last_seen_at.to_rfc3339())
    .bind(m.missing_since.map(|t| t.to_rfc3339()))
    .bind(m.status.as_str())
    .execute(pool)
    .await?;
    Ok(m.clone())
}

pub async fn list_catalog_models(
    pool: &Pool<Sqlite>,
    endpoint_id: Uuid,
) -> Result<Vec<ProviderCatalogModel>, DbError> {
    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            Option<String>,
            Option<i64>,
            String,
            String,
            Option<String>,
            String,
        ),
    >(
        "SELECT id, endpoint_id, remote_model_id, raw_metadata_json, canonical_model_id,
                match_confidence, first_seen_at, last_seen_at, missing_since, status
         FROM provider_catalog_models WHERE endpoint_id = ? ORDER BY remote_model_id",
    )
    .bind(endpoint_id.to_string())
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(
            |(id, eid, rid, raw, canon, conf, first, last, missing, status)| {
                Ok(ProviderCatalogModel {
                    id: Uuid::parse_str(&id).map_err(|_| DbError::NotFound(id))?,
                    endpoint_id: Uuid::parse_str(&eid).map_err(|_| DbError::NotFound(eid))?,
                    remote_model_id: rid,
                    raw_metadata: serde_json::from_str(&raw).unwrap_or_default(),
                    canonical_model_id: canon
                        .map(|c| Uuid::parse_str(&c).map_err(|_| DbError::NotFound(c.clone())))
                        .transpose()?,
                    match_confidence: conf.map(|c| c as u8),
                    first_seen_at: parse_ts(&first),
                    last_seen_at: parse_ts(&last),
                    missing_since: missing.map(|t| parse_ts(&t)),
                    status: CatalogStatus::parse_str(&status),
                })
            },
        )
        .collect()
}

pub async fn create_route(pool: &Pool<Sqlite>, r: &ModelRoute) -> Result<ModelRoute, DbError> {
    sqlx::query(
        "INSERT INTO model_routes
           (id, endpoint_id, model_identity_id, remote_model_id, display_name,
            context_window, max_input, max_output, capabilities_json, overrides_json,
            enabled, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(r.id.to_string())
    .bind(r.endpoint_id.to_string())
    .bind(r.model_identity_id.map(|i| i.to_string()))
    .bind(&r.remote_model_id)
    .bind(&r.display_name)
    .bind(r.context_window)
    .bind(r.max_input)
    .bind(r.max_output)
    .bind(serde_json::to_string(&r.capabilities)?)
    .bind(serde_json::to_string(&r.overrides)?)
    .bind(r.enabled as i64)
    .bind(r.created_at.to_rfc3339())
    .bind(r.updated_at.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(r.clone())
}

pub async fn update_route(pool: &Pool<Sqlite>, r: &ModelRoute) -> Result<ModelRoute, DbError> {
    let res = sqlx::query(
        "UPDATE model_routes SET
           model_identity_id = ?, display_name = ?, context_window = ?, max_input = ?,
           max_output = ?, capabilities_json = ?, overrides_json = ?, enabled = ?, updated_at = ?
         WHERE id = ?",
    )
    .bind(r.model_identity_id.map(|i| i.to_string()))
    .bind(&r.display_name)
    .bind(r.context_window)
    .bind(r.max_input)
    .bind(r.max_output)
    .bind(serde_json::to_string(&r.capabilities)?)
    .bind(serde_json::to_string(&r.overrides)?)
    .bind(r.enabled as i64)
    .bind(Utc::now().to_rfc3339())
    .bind(r.id.to_string())
    .execute(pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(DbError::NotFound(format!("route {}", r.id)));
    }
    Ok(r.clone())
}

pub async fn delete_route(pool: &Pool<Sqlite>, id: Uuid) -> Result<(), DbError> {
    let res = sqlx::query("DELETE FROM model_routes WHERE id = ?")
        .bind(id.to_string())
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(DbError::NotFound(format!("route {id}")));
    }
    Ok(())
}

pub async fn list_routes(pool: &Pool<Sqlite>) -> Result<Vec<ModelRoute>, DbError> {
    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            Option<String>,
            String,
            String,
            Option<i64>,
            Option<i64>,
            Option<i64>,
            String,
            String,
            i64,
            String,
            String,
        ),
    >(
        "SELECT id, endpoint_id, model_identity_id, remote_model_id, display_name,
                context_window, max_input, max_output, capabilities_json, overrides_json,
                enabled, created_at, updated_at
         FROM model_routes ORDER BY display_name",
    )
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(
            |(id, eid, mid, rid, dname, ctx, mi, mo, caps, ovr, enabled, created, updated)| {
                Ok(ModelRoute {
                    id: Uuid::parse_str(&id).map_err(|_| DbError::NotFound(id))?,
                    endpoint_id: Uuid::parse_str(&eid).map_err(|_| DbError::NotFound(eid))?,
                    model_identity_id: mid
                        .map(|m| Uuid::parse_str(&m).map_err(|_| DbError::NotFound(m.clone())))
                        .transpose()?,
                    remote_model_id: rid,
                    display_name: dname,
                    context_window: ctx,
                    max_input: mi,
                    max_output: mo,
                    capabilities: serde_json::from_str(&caps).unwrap_or_default(),
                    overrides: serde_json::from_str(&ovr).unwrap_or_default(),
                    enabled: enabled == 1,
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
