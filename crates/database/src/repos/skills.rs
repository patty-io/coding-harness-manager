//! Skill + harness skill binding repositories.

use chm_core::domain::harness::{BindingType, HarnessSkillBinding};
use chm_core::domain::skills::{Skill, SkillSourceType};
use chrono::Utc;
use sqlx::{Pool, Sqlite};
use uuid::Uuid;

use crate::DbError;

pub async fn create_skill(pool: &Pool<Sqlite>, s: &Skill) -> Result<Skill, DbError> {
    sqlx::query(
        "INSERT INTO skills
           (id, name, canonical_path, source_type, source_url, content_hash,
            provenance_json, enabled, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(s.id.to_string())
    .bind(&s.name)
    .bind(&s.canonical_path)
    .bind(s.source_type.as_str())
    .bind(&s.source_url)
    .bind(&s.content_hash)
    .bind(serde_json::to_string(&s.provenance)?)
    .bind(s.enabled as i64)
    .bind(s.created_at.to_rfc3339())
    .bind(s.updated_at.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(s.clone())
}

pub async fn list_skills(pool: &Pool<Sqlite>) -> Result<Vec<Skill>, DbError> {
    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            String,
            i64,
            String,
            String,
        ),
    >(
        "SELECT id, name, canonical_path, source_type, source_url, content_hash,
                provenance_json, enabled, created_at, updated_at
         FROM skills ORDER BY name",
    )
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(
            |(id, name, path, source, url, hash, provenance, enabled, created, updated)| {
                Ok(Skill {
                    id: Uuid::parse_str(&id).map_err(|_| DbError::NotFound(id))?,
                    name,
                    canonical_path: path,
                    source_type: SkillSourceType::parse_str(&source),
                    source_url: url,
                    content_hash: hash,
                    provenance: serde_json::from_str(&provenance).unwrap_or_default(),
                    enabled: enabled == 1,
                    created_at: parse_ts(&created),
                    updated_at: parse_ts(&updated),
                })
            },
        )
        .collect()
}

pub async fn create_skill_binding(
    pool: &Pool<Sqlite>,
    b: &HarnessSkillBinding,
) -> Result<(), DbError> {
    sqlx::query(
        "INSERT INTO harness_skill_bindings
           (id, harness_installation_id, skill_id, target_path, binding_type, managed, status)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(b.id.to_string())
    .bind(b.harness_installation_id.to_string())
    .bind(b.skill_id.to_string())
    .bind(&b.target_path)
    .bind(b.binding_type.as_str())
    .bind(b.managed as i64)
    .bind(&b.status)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_skill_bindings(
    pool: &Pool<Sqlite>,
    installation_id: Uuid,
) -> Result<Vec<HarnessSkillBinding>, DbError> {
    let rows = sqlx::query_as::<_, (String, String, String, String, String, i64, String)>(
        "SELECT id, harness_installation_id, skill_id, target_path, binding_type, managed, status
         FROM harness_skill_bindings WHERE harness_installation_id = ?",
    )
    .bind(installation_id.to_string())
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|(id, hid, sid, target, binding, managed, status)| {
            Ok(HarnessSkillBinding {
                id: Uuid::parse_str(&id).map_err(|_| DbError::NotFound(id))?,
                harness_installation_id: Uuid::parse_str(&hid)
                    .map_err(|_| DbError::NotFound(hid))?,
                skill_id: Uuid::parse_str(&sid).map_err(|_| DbError::NotFound(sid))?,
                target_path: target,
                binding_type: BindingType::parse_str(&binding),
                managed: managed == 1,
                status,
            })
        })
        .collect()
}

fn parse_ts(s: &str) -> chrono::DateTime<Utc> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}
