//! Launch profile + configuration set repositories.

use chm_core::domain::harness::HarnessType;
use chm_core::domain::profiles::LaunchProfile;
use chm_core::domain::sets::{ConfigurationSet, ConfigurationSetItem, SetItemType};
use chm_core::parse_ts;
use chrono::Utc;
use sqlx::{Pool, Sqlite};
use std::collections::HashMap;
use uuid::Uuid;

use crate::DbError;

pub async fn create_profile<'e>(
    pool: impl sqlx::Executor<'e, Database = Sqlite> + 'e,
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

pub async fn update_profile(
    pool: &Pool<Sqlite>,
    p: &LaunchProfile,
) -> Result<LaunchProfile, DbError> {
    let result = sqlx::query(
        "UPDATE launch_profiles SET name = ?, harness_type = ?, model_route_id = ?,
            provider_endpoint_id = ?, env_json = ?, role_mappings_json = ?,
            native_overrides_json = ?, updated_at = ? WHERE id = ?",
    )
    .bind(&p.name)
    .bind(p.harness_type.as_str())
    .bind(p.model_route_id.map(|id| id.to_string()))
    .bind(p.provider_endpoint_id.map(|id| id.to_string()))
    .bind(serde_json::to_string(&p.env)?)
    .bind(serde_json::to_string(&p.role_mappings)?)
    .bind(serde_json::to_string(&p.native_overrides)?)
    .bind(p.updated_at.to_rfc3339())
    .bind(p.id.to_string())
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        return Err(DbError::NotFound(format!("launch profile {}", p.id)));
    }
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
                    created_at: parse_ts(&created)?,
                    updated_at: parse_ts(&updated)?,
                })
            },
        )
        .collect()
}

pub async fn create_set<'e>(
    pool: impl sqlx::Executor<'e, Database = Sqlite> + 'e,
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

pub async fn update_set(
    pool: &Pool<Sqlite>,
    set: &ConfigurationSet,
) -> Result<ConfigurationSet, DbError> {
    let result =
        sqlx::query("UPDATE configuration_sets SET description = ?, updated_at = ? WHERE id = ?")
            .bind(&set.description)
            .bind(set.updated_at.to_rfc3339())
            .bind(set.id.to_string())
            .execute(pool)
            .await?;
    if result.rows_affected() == 0 {
        return Err(DbError::NotFound(format!("configuration set {}", set.id)));
    }
    Ok(set.clone())
}

pub async fn clear_set_items(pool: &Pool<Sqlite>, set_id: Uuid) -> Result<(), DbError> {
    sqlx::query("DELETE FROM configuration_set_items WHERE configuration_set_id = ?")
        .bind(set_id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn add_set_item<'e>(
    pool: impl sqlx::Executor<'e, Database = Sqlite> + 'e,
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

pub async fn remove_set_item(
    pool: &Pool<Sqlite>,
    set_id: Uuid,
    item_type: SetItemType,
    item_id: Uuid,
) -> Result<(), DbError> {
    sqlx::query(
        "DELETE FROM configuration_set_items
         WHERE configuration_set_id = ? AND item_type = ? AND item_id = ?",
    )
    .bind(set_id.to_string())
    .bind(item_type.as_str())
    .bind(item_id.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_set(pool: &Pool<Sqlite>, set_id: Uuid) -> Result<(), DbError> {
    let result = sqlx::query("DELETE FROM configuration_sets WHERE id = ?")
        .bind(set_id.to_string())
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(DbError::NotFound(format!("configuration set {set_id}")));
    }
    Ok(())
}

pub async fn list_set_items<'e>(
    pool: impl sqlx::Executor<'e, Database = Sqlite> + 'e,
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

pub async fn list_set_items_for_sets(
    pool: &Pool<Sqlite>,
    set_ids: &[Uuid],
) -> Result<HashMap<Uuid, Vec<ConfigurationSetItem>>, DbError> {
    if set_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let placeholders = std::iter::repeat_n("?", set_ids.len())
        .collect::<Vec<_>>()
        .join(",");
    let query = format!(
        "SELECT id, configuration_set_id, item_type, item_id
         FROM configuration_set_items WHERE configuration_set_id IN ({placeholders})"
    );
    let mut request = sqlx::query_as::<_, (String, String, String, String)>(&query);
    for set_id in set_ids {
        request = request.bind(set_id.to_string());
    }
    let rows = request.fetch_all(pool).await?;
    let mut grouped: HashMap<Uuid, Vec<ConfigurationSetItem>> = HashMap::new();
    for (id, sid, item_type, item_id) in rows {
        let set_id = Uuid::parse_str(&sid).map_err(|_| DbError::NotFound(sid))?;
        grouped
            .entry(set_id)
            .or_default()
            .push(ConfigurationSetItem {
                id: Uuid::parse_str(&id).map_err(|_| DbError::NotFound(id))?,
                configuration_set_id: set_id,
                item_type: SetItemType::parse_str(&item_type),
                item_id: Uuid::parse_str(&item_id).map_err(|_| DbError::NotFound(item_id))?,
            });
    }
    Ok(grouped)
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
                created_at: parse_ts(&created)?,
                updated_at: parse_ts(&updated)?,
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
