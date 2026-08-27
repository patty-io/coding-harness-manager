//! Provider, endpoint, and credential-ref repositories.

use chm_core::domain::credentials::{CredentialKind, CredentialRef};
use chm_core::domain::provider::{AuthType, Protocol, Provider, ProviderEndpoint};
use chm_core::parse_ts;
use chrono::Utc;
use sqlx::{Pool, Sqlite};
use uuid::Uuid;

use crate::DbError;

pub async fn create_provider<'e>(
    pool: impl sqlx::Executor<'e, Database = Sqlite> + 'e,
    name: &str,
    display_name: &str,
) -> Result<Provider, DbError> {
    let now = Utc::now();
    let p = Provider {
        id: Uuid::new_v4(),
        name: name.into(),
        display_name: display_name.into(),
        enabled: true,
        notes: None,
        created_at: now,
        updated_at: now,
    };
    sqlx::query(
        "INSERT INTO providers (id, name, display_name, enabled, notes, created_at, updated_at)
         VALUES (?, ?, ?, 1, NULL, ?, ?)",
    )
    .bind(p.id.to_string())
    .bind(&p.name)
    .bind(&p.display_name)
    .bind(p.created_at.to_rfc3339())
    .bind(p.updated_at.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(p)
}

pub async fn list_providers(pool: &Pool<Sqlite>) -> Result<Vec<Provider>, DbError> {
    let rows = sqlx::query_as::<_, (String, String, String, i64, Option<String>, String, String)>(
        "SELECT id, name, display_name, enabled, notes, created_at, updated_at
         FROM providers ORDER BY name",
    )
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(row_to_provider).collect()
}

pub async fn update_provider(
    pool: &Pool<Sqlite>,
    id: Uuid,
    display_name: &str,
    enabled: bool,
    notes: Option<String>,
) -> Result<Provider, DbError> {
    let now = Utc::now();
    let res = sqlx::query(
        "UPDATE providers SET display_name = ?, enabled = ?, notes = ?, updated_at = ?
         WHERE id = ?",
    )
    .bind(display_name)
    .bind(enabled as i64)
    .bind(&notes)
    .bind(now.to_rfc3339())
    .bind(id.to_string())
    .execute(pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(DbError::NotFound(format!("provider {id}")));
    }
    Ok(Provider {
        id,
        name: String::new(),
        display_name: display_name.into(),
        enabled,
        notes,
        created_at: now,
        updated_at: now,
    })
}

pub async fn delete_provider<'e>(
    pool: impl sqlx::Executor<'e, Database = Sqlite> + 'e,
    id: Uuid,
) -> Result<(), DbError> {
    let res = sqlx::query("DELETE FROM providers WHERE id = ?")
        .bind(id.to_string())
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(DbError::NotFound(format!("provider {id}")));
    }
    Ok(())
}

pub async fn create_credential_ref<'e>(
    pool: impl sqlx::Executor<'e, Database = Sqlite> + 'e,
    kind: CredentialKind,
    reference: &str,
) -> Result<CredentialRef, DbError> {
    let now = Utc::now();
    let c = CredentialRef {
        id: Uuid::new_v4(),
        kind,
        reference: reference.into(),
        created_at: now,
        updated_at: now,
    };
    sqlx::query(
        "INSERT INTO credential_refs (id, type, reference, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(c.id.to_string())
    .bind(c.kind.as_str())
    .bind(&c.reference)
    .bind(c.created_at.to_rfc3339())
    .bind(c.updated_at.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(c)
}

pub async fn get_credential_ref<'e>(
    pool: impl sqlx::Executor<'e, Database = Sqlite> + 'e,
    id: Uuid,
) -> Result<CredentialRef, DbError> {
    let (kind, reference, created, updated) =
        sqlx::query_as::<_, (String, String, String, String)>(
            "SELECT type, reference, created_at, updated_at FROM credential_refs WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_one(pool)
        .await?;
    Ok(CredentialRef {
        id,
        kind: CredentialKind::parse_str(&kind),
        reference,
        created_at: parse_ts(&created)?,
        updated_at: parse_ts(&updated)?,
    })
}

pub async fn create_endpoint<'e>(
    pool: impl sqlx::Executor<'e, Database = Sqlite> + 'e,
    e: &ProviderEndpoint,
) -> Result<ProviderEndpoint, DbError> {
    sqlx::query(
        "INSERT INTO provider_endpoints
           (id, provider_id, name, base_url, protocol, discovery_path, auth_type,
            credential_ref_id, headers_json, enabled, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(e.id.to_string())
    .bind(e.provider_id.to_string())
    .bind(&e.name)
    .bind(&e.base_url)
    .bind(e.protocol.as_str())
    .bind(&e.discovery_path)
    .bind(e.auth_type.as_str())
    .bind(e.credential_ref.as_ref().map(|c| c.id.to_string()))
    .bind(serde_json::to_string(&e.headers)?)
    .bind(e.enabled as i64)
    .bind(e.created_at.to_rfc3339())
    .bind(e.updated_at.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(e.clone())
}

pub async fn list_endpoints<'e>(
    pool: impl sqlx::Executor<'e, Database = Sqlite> + 'e,
    provider_id: Uuid,
) -> Result<Vec<ProviderEndpoint>, DbError> {
    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            String,
            Option<String>,
            String,
            Option<String>,
            String,
            i64,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        ),
    >(
        "SELECT e.id, e.provider_id, e.name, e.base_url, e.protocol, e.discovery_path,
                e.auth_type, e.credential_ref_id, e.headers_json, e.enabled,
                e.created_at, e.updated_at,
                c.type, c.reference, c.created_at, c.updated_at
         FROM provider_endpoints e
         LEFT JOIN credential_refs c ON c.id = e.credential_ref_id
         WHERE e.provider_id = ? ORDER BY e.name",
    )
    .bind(provider_id.to_string())
    .fetch_all(pool)
    .await?;
    let mut out = Vec::new();
    for (
        id,
        pid,
        name,
        base_url,
        protocol,
        discovery,
        auth,
        cred_id,
        headers,
        enabled,
        created,
        updated,
        ctype,
        cref,
        ccreated,
        cupdated,
    ) in rows
    {
        let credential_ref = match (cred_id, ctype, cref, ccreated, cupdated) {
            (Some(cid), Some(kind), Some(reference), Some(ccreated), Some(cupdated)) => {
                Some(CredentialRef {
                    id: Uuid::parse_str(&cid).map_err(|_| DbError::InvalidData(cid))?,
                    kind: CredentialKind::parse_str(&kind),
                    reference,
                    created_at: parse_ts(&ccreated)?,
                    updated_at: parse_ts(&cupdated)?,
                })
            }
            (Some(_), ..) => {
                return Err(DbError::InvalidData("credential ref row incomplete".into()));
            }
            (None, ..) => None,
        };
        out.push(ProviderEndpoint {
            id: Uuid::parse_str(&id).map_err(|_| DbError::InvalidData(id))?,
            provider_id: Uuid::parse_str(&pid).map_err(|_| DbError::InvalidData(pid))?,
            name,
            base_url,
            protocol: Protocol::parse_str(&protocol),
            discovery_path: discovery,
            auth_type: AuthType::parse_str(&auth),
            credential_ref,
            headers: serde_json::from_str(&headers).unwrap_or_default(),
            enabled: enabled == 1,
            created_at: parse_ts(&created)?,
            updated_at: parse_ts(&updated)?,
        });
    }
    Ok(out)
}

fn row_to_provider(
    (id, name, display_name, enabled, notes, created, updated): (
        String,
        String,
        String,
        i64,
        Option<String>,
        String,
        String,
    ),
) -> Result<Provider, DbError> {
    Ok(Provider {
        id: Uuid::parse_str(&id).map_err(|_| DbError::NotFound(id))?,
        name,
        display_name,
        enabled: enabled == 1,
        notes,
        created_at: parse_ts(&created)?,
        updated_at: parse_ts(&updated)?,
    })
}
