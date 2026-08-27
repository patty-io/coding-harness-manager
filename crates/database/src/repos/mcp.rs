//! Canonical MCP server + harness binding repositories.

use chm_core::domain::harness::HarnessMcpBinding;
use chm_core::domain::mcp::{McpServer, McpTransport, ScopeType};
use sqlx::{Pool, Sqlite};
use uuid::Uuid;

use crate::DbError;

pub async fn create_mcp_server<'e>(
    pool: impl sqlx::Executor<'e, Database = Sqlite> + 'e,
    s: &McpServer,
) -> Result<McpServer, DbError> {
    sqlx::query(
        "INSERT INTO mcp_servers
           (id, name, transport, command, args_json, url, env_json, scope_type, scope_path,
            provenance_json, enabled)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(s.id.to_string())
    .bind(&s.name)
    .bind(s.transport.as_str())
    .bind(&s.command)
    .bind(serde_json::to_string(&s.args)?)
    .bind(&s.url)
    .bind(serde_json::to_string(&s.env)?)
    .bind(s.scope_type.as_str())
    .bind(&s.scope_path)
    .bind(serde_json::to_string(&s.provenance)?)
    .bind(s.enabled as i64)
    .execute(pool)
    .await?;
    Ok(s.clone())
}

pub async fn list_mcp_servers(pool: &Pool<Sqlite>) -> Result<Vec<McpServer>, DbError> {
    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            Option<String>,
            String,
            Option<String>,
            String,
            String,
            Option<String>,
            String,
            i64,
        ),
    >(
        "SELECT id, name, transport, command, args_json, url, env_json, scope_type,
                scope_path, provenance_json, enabled
         FROM mcp_servers ORDER BY name",
    )
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(
            |(
                id,
                name,
                transport,
                command,
                args,
                url,
                env,
                scope,
                scope_path,
                provenance,
                enabled,
            )| {
                Ok(McpServer {
                    id: Uuid::parse_str(&id).map_err(|_| DbError::NotFound(id))?,
                    name,
                    transport: McpTransport::parse_str(&transport),
                    command,
                    args: serde_json::from_str(&args).unwrap_or_default(),
                    url,
                    env: serde_json::from_str(&env).unwrap_or_default(),
                    scope_type: ScopeType::parse_str(&scope),
                    scope_path,
                    provenance: serde_json::from_str(&provenance).unwrap_or_default(),
                    enabled: enabled == 1,
                })
            },
        )
        .collect()
}

pub async fn delete_mcp_server<'e>(
    pool: impl sqlx::Executor<'e, Database = Sqlite> + 'e,
    id: Uuid,
) -> Result<(), DbError> {
    let res = sqlx::query("DELETE FROM mcp_servers WHERE id = ?")
        .bind(id.to_string())
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(DbError::NotFound(format!("mcp server {id}")));
    }
    Ok(())
}

pub async fn create_mcp_binding<'e>(
    pool: impl sqlx::Executor<'e, Database = Sqlite> + 'e,
    b: &HarnessMcpBinding,
) -> Result<(), DbError> {
    sqlx::query(
        "INSERT INTO harness_mcp_bindings
           (id, harness_installation_id, mcp_server_id, native_name, native_config_json, managed)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(b.id.to_string())
    .bind(b.harness_installation_id.to_string())
    .bind(b.mcp_server_id.to_string())
    .bind(&b.native_name)
    .bind(serde_json::to_string(&b.native_config)?)
    .bind(b.managed as i64)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_mcp_bindings<'e>(
    pool: impl sqlx::Executor<'e, Database = Sqlite> + 'e,
    installation_id: Uuid,
) -> Result<Vec<HarnessMcpBinding>, DbError> {
    let rows = sqlx::query_as::<_, (String, String, String, String, String, i64)>(
        "SELECT id, harness_installation_id, mcp_server_id, native_name, native_config_json, managed
         FROM harness_mcp_bindings WHERE harness_installation_id = ?",
    )
    .bind(installation_id.to_string())
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|(id, hid, sid, native_name, native_config, managed)| {
            Ok(HarnessMcpBinding {
                id: Uuid::parse_str(&id).map_err(|_| DbError::NotFound(id))?,
                harness_installation_id: Uuid::parse_str(&hid)
                    .map_err(|_| DbError::NotFound(hid))?,
                mcp_server_id: Uuid::parse_str(&sid).map_err(|_| DbError::NotFound(sid))?,
                native_name,
                native_config: serde_json::from_str(&native_config).unwrap_or_default(),
                managed: managed == 1,
            })
        })
        .collect()
}
