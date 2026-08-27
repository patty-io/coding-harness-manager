//! Launch profile + configuration set repositories.

use chm_core::domain::harness::HarnessType;
use chm_core::domain::profiles::LaunchProfile;
use chm_core::domain::sets::{ConfigurationSet, ConfigurationSetItem, SetItemType};
use chrono::Utc;
use sqlx::{Pool, Sqlite};
use uuid::Uuid;

use crate::DbError;

pub async fn create_profile(
    pool: &Pool<Sqlite>,
    p: &LaunchProfile,
) -> Result<LaunchProfile, DbError> {
    sqlx::query(
        "INSERT INTO launch_profiles
           (id, name, harness_type, model_route_id, provider_endpoint_id, env_json,
            role_mappings_json, native_overrides_json, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(p.id.to_string())
    .bind(&p.name)
    .bind(p.harness_type.as_str())
    .bind(p.model_route_id.map(|i| i.to_string()))
    .bind(p.provider_endpoint_id.map(|i| i.to_string()))
    .bind(serde_json::to_string(&p.env)?)
    .bind(serde_json::to_string(&p.role_mappings)?)
    .bind(serde_json::to_string(&p.native_overrides)?)
    .bind(p.created_at.to_rfc3339())
    .bind(p.updated_at.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(p.clone())
}

pub async fn list_profiles(pool: &Pool<Sqlite>) -> Result<Vec<LaunchProfile>, DbError> {
    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            String,
            String,
            String,
            String,
            String,
        ),
    >(
        "SELECT id, name, harness_type, model_route_id, provider_endpoint_id, env_json,
                role_mappings_json, native_overrides_json, created_at, updated_at
         FROM launch_profiles ORDER BY name",
    )
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(
            |(
                id,
                name,
                harness,
                route_id,
                endpoint_id,
                env,
                roles,
                overrides,
                created,
                updated,
            )| {
                Ok(LaunchProfile {
                    id: Uuid::parse_str(&id).map_err(|_| DbError::NotFound(id))?,
                    name,
                    harness_type: HarnessType::parse_str(&harness),
                    model_route_id: parse_opt_uuid(route_id)?,
                    provider_endpoint_id: parse_opt_uuid(endpoint_id)?,
                    env: serde_json::from_str(&env).unwrap_or_default(),
                    role_mappings: serde_json::from_str(&roles).unwrap_or_default(),
                    native_overrides: serde_json::from_str(&overrides).unwrap_or_default(),
                    created_at: parse_ts(&created),
                    updated_at: parse_ts(&updated),
                })
            },
        )
        .collect()
}

pub async fn create_set(
    pool: &Pool<Sqlite>,
    name: &str,
    description: Option<String>,
) -> Result<ConfigurationSet, DbError> {
    let now = Utc::now();
    let set = ConfigurationSet {
        id: Uuid::new_v4(),
        name: name.into(),
        description,
        created_at: now,
        updated_at: now,
    };
    sqlx::query(
        "INSERT INTO configuration_sets (id, name, description, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(set.id.to_string())
    .bind(&set.name)
    .bind(&set.description)
    .bind(set.created_at.to_rfc3339())
    .bind(set.updated_at.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(set)
}

pub async fn add_set_item(
    pool: &Pool<Sqlite>,
    set_id: Uuid,
    item_type: SetItemType,
    item_id: Uuid,
) -> Result<(), DbError> {
    sqlx::query(
        "INSERT INTO configuration_set_items (id, configuration_set_id, item_type, item_id)
         VALUES (?, ?, ?, ?)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(set_id.to_string())
    .bind(item_type.as_str())
    .bind(item_id.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_set_items(
    pool: &Pool<Sqlite>,
    set_id: Uuid,
) -> Result<Vec<ConfigurationSetItem>, DbError> {
    let rows = sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT id, configuration_set_id, item_type, item_id
         FROM configuration_set_items WHERE configuration_set_id = ?",
    )
    .bind(set_id.to_string())
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|(id, sid, item_type, item_id)| {
            Ok(ConfigurationSetItem {
                id: Uuid::parse_str(&id).map_err(|_| DbError::NotFound(id))?,
                configuration_set_id: Uuid::parse_str(&sid).map_err(|_| DbError::NotFound(sid))?,
                item_type: SetItemType::parse_str(&item_type),
                item_id: Uuid::parse_str(&item_id).map_err(|_| DbError::NotFound(item_id))?,
            })
        })
        .collect()
}

pub async fn list_sets(pool: &Pool<Sqlite>) -> Result<Vec<ConfigurationSet>, DbError> {
    let rows = sqlx::query_as::<_, (String, String, Option<String>, String, String)>(
        "SELECT id, name, description, created_at, updated_at
         FROM configuration_sets ORDER BY name",
    )
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|(id, name, description, created, updated)| {
            Ok(ConfigurationSet {
                id: Uuid::parse_str(&id).map_err(|_| DbError::NotFound(id))?,
                name,
                description,
                created_at: parse_ts(&created),
                updated_at: parse_ts(&updated),
            })
        })
        .collect()
}

fn parse_opt_uuid(s: Option<String>) -> Result<Option<Uuid>, DbError> {
    match s {
        Some(v) => Ok(Some(Uuid::parse_str(&v).map_err(|_| DbError::NotFound(v))?)),
        None => Ok(None),
    }
}

fn parse_ts(s: &str) -> chrono::DateTime<Utc> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}
