//! Database backup and portable configuration export/import commands.
//!
//! The database backup is a byte-for-byte SQLite snapshot. Portable exports
//! are deliberately a separate, credential-safe JSON format: credential
//! references are preserved, but secret values are never read from the
//! secret store.

use chm_core::domain::mcp::McpServer;
use chm_core::domain::models::ModelRoute;
use chm_core::domain::profiles::LaunchProfile;
use chm_core::domain::provider::{Provider, ProviderEndpoint};
use chm_core::domain::sets::{ConfigurationSet, ConfigurationSetItem};
use chm_core::domain::skills::Skill;
use chm_database::repos::history::{begin_transaction, finish_transaction};
use chm_database::repos::mcp::{create_mcp_server, list_mcp_servers, update_mcp_server};
use chm_database::repos::models::{create_route, list_routes, update_route};
use chm_database::repos::profiles::{
    add_set_item, clear_set_items, create_profile, create_set, list_profiles, list_set_items,
    list_set_items_for_sets, list_sets, update_profile, update_set,
};
use chm_database::repos::providers::{
    create_credential_ref, create_endpoint, create_provider, list_endpoints, list_providers,
    update_endpoint, update_provider,
};
use chm_database::repos::skills::{create_skill, list_skills, update_skill};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Pool, Row, Sqlite};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tauri::State;
use uuid::Uuid;

use crate::AppState;
use chm_core::domain::history::{TransactionStatus, TransactionType};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortableExport {
    pub schema_version: u32,
    pub app_version: String,
    pub exported_at: String,
    pub providers: Vec<Provider>,
    pub endpoints: Vec<ProviderEndpoint>,
    pub model_routes: Vec<ModelRoute>,
    pub mcp_servers: Vec<McpServer>,
    pub skills: Vec<Skill>,
    pub launch_profiles: Vec<LaunchProfile>,
    pub configuration_sets: Vec<ConfigurationSetExport>,
    pub preferences: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigurationSetExport {
    pub set: ConfigurationSet,
    pub items: Vec<ConfigurationSetItem>,
}

fn destination_dir(value: &str) -> PathBuf {
    if value.trim().is_empty() {
        crate::app_data_dir()
    } else {
        crate::expand_user_path(value.trim())
    }
}

fn normalize_url(value: &str) -> String {
    value.trim().trim_end_matches('/').to_lowercase()
}

fn is_secret_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "secret",
        "token",
        "password",
        "passwd",
        "api_key",
        "apikey",
        "access_key",
        "private_key",
        "authorization",
        "cookie",
    ]
    .iter()
    .any(|needle| key.contains(needle))
}

/// Portable exports must never copy values that can authenticate to a
/// provider. Keep the shape (and therefore useful diagnostics) but replace
/// values for conventionally secret-looking keys.
fn redact_value(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        if is_secret_key(key) {
                            Value::String("<redacted>".into())
                        } else {
                            redact_value(value)
                        },
                    )
                })
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(redact_value).collect()),
        other => other.clone(),
    }
}

fn redacted_map(map: &serde_json::Map<String, Value>) -> serde_json::Map<String, Value> {
    match redact_value(&Value::Object(map.clone())) {
        Value::Object(map) => map,
        _ => serde_json::Map::new(),
    }
}

/// Compare an exported value with live state while treating the explicit
/// redaction marker as a wildcard. This keeps a portable export idempotent:
/// a preserved local secret should not appear as a false conflict merely
/// because the export intentionally omitted its value.
fn portable_value_equivalent(exported: &Value, current: &Value) -> bool {
    if exported == &Value::String("<redacted>".into()) {
        return true;
    }
    match (exported, current) {
        (Value::Object(left), Value::Object(right)) => {
            left.len() == right.len()
                && left.iter().all(|(key, value)| {
                    right
                        .get(key)
                        .is_some_and(|current| portable_value_equivalent(value, current))
                })
        }
        (Value::Array(left), Value::Array(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| portable_value_equivalent(left, right))
        }
        _ => exported == current,
    }
}

fn portable_map_equivalent(
    exported: &serde_json::Map<String, Value>,
    current: &serde_json::Map<String, Value>,
) -> bool {
    portable_value_equivalent(
        &Value::Object(exported.clone()),
        &Value::Object(current.clone()),
    )
}

fn provider_equivalent(left: &Provider, right: &Provider) -> bool {
    left.name.eq_ignore_ascii_case(&right.name)
        && left.display_name == right.display_name
        && left.enabled == right.enabled
        && left.notes == right.notes
}

fn endpoint_equivalent(left: &ProviderEndpoint, right: &ProviderEndpoint) -> bool {
    left.name == right.name
        && normalize_url(&left.base_url) == normalize_url(&right.base_url)
        && left.protocol == right.protocol
        && left.discovery_path == right.discovery_path
        && left.auth_type == right.auth_type
        && portable_map_equivalent(&left.headers, &right.headers)
        && left.enabled == right.enabled
}

fn route_equivalent(left: &ModelRoute, right: &ModelRoute) -> bool {
    left.remote_model_id == right.remote_model_id
        && left.display_name == right.display_name
        && left.context_window == right.context_window
        && left.max_input == right.max_input
        && left.max_output == right.max_output
        && portable_value_equivalent(&left.capabilities, &right.capabilities)
        && portable_value_equivalent(&left.overrides, &right.overrides)
        && left.enabled == right.enabled
}

fn mcp_equivalent(left: &McpServer, right: &McpServer) -> bool {
    left.name == right.name
        && left.transport == right.transport
        && left.command == right.command
        && left.args == right.args
        && left.url == right.url
        && portable_map_equivalent(&left.env, &right.env)
        && left.scope_type == right.scope_type
        && left.scope_path == right.scope_path
        && left.enabled == right.enabled
}

fn skill_equivalent(left: &Skill, right: &Skill) -> bool {
    left.name == right.name
        && left.canonical_path == right.canonical_path
        && left.source_type == right.source_type
        && left.source_url == right.source_url
        && left.content_hash == right.content_hash
        && left.enabled == right.enabled
}

async fn database_path(pool: &Pool<Sqlite>) -> Result<PathBuf, String> {
    let rows = sqlx::query("PRAGMA database_list")
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
    rows.into_iter()
        .find(|row| row.try_get::<String, _>(1).ok().as_deref() == Some("main"))
        .and_then(|row| row.try_get::<String, _>(2).ok())
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| "could not determine the live SQLite database path".into())
}

/// Create a consistent SQLite snapshot with VACUUM INTO. The destination is
/// created inside the chosen directory and never overwrites an existing file.
async fn backup_now_inner(pool: &Pool<Sqlite>, dest_dir: &str) -> Result<String, String> {
    let source = database_path(pool).await?;
    let destination = destination_dir(dest_dir);
    std::fs::create_dir_all(&destination).map_err(|e| e.to_string())?;
    let stamp = Utc::now().format("%Y%m%dT%H%M%S");
    let mut path = destination.join(format!("chm-backup-{stamp}.sqlite"));
    let mut suffix = 2;
    while path.exists() {
        path = destination.join(format!("chm-backup-{stamp}-{suffix}.sqlite"));
        suffix += 1;
    }
    if path == source {
        return Err("backup destination must not be the live database".into());
    }
    let escaped = path.to_string_lossy().replace('\'', "''");
    sqlx::query(&format!("VACUUM INTO '{}'", escaped))
        .execute(pool)
        .await
        .map_err(|e| format!("database backup failed: {e}"))?;
    Ok(path.display().to_string())
}

pub fn list_backups_core(dest_dir: &str) -> Result<Vec<String>, String> {
    let destination = destination_dir(dest_dir);
    if !destination.is_dir() {
        return Ok(Vec::new());
    }
    let mut paths = std::fs::read_dir(&destination)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        name.starts_with("chm-backup-") && name.ends_with(".sqlite")
                    })
        })
        .collect::<Vec<_>>();
    paths.sort();
    Ok(paths
        .into_iter()
        .rev()
        .map(|path| path.display().to_string())
        .collect())
}

struct RestoreOutcome {
    message: String,
    live_path: PathBuf,
}

/// Open a short-lived connection to the restored database. The application's
/// pool is deliberately closed before replacing the file, so the audit row
/// must be completed through a fresh connection afterward.
async fn open_restore_pool(path: &Path) -> Result<Pool<Sqlite>, String> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(false)
        .foreign_keys(true);
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|e| format!("could not reopen restored database: {e}"))
}

async fn finish_restore_audit(
    live_path: &Path,
    audit_id: Uuid,
    status: TransactionStatus,
    summary: Option<String>,
    error: Option<String>,
) -> Result<(), String> {
    let audit_pool = open_restore_pool(live_path).await?;
    // A backup made before this restore began will not contain the original
    // running audit row. In that case create a completion row in the restored
    // database rather than silently losing the audit trail.
    if finish_transaction(&audit_pool, audit_id, status, summary.clone(), error.clone())
        .await
        .is_err()
    {
        let replacement = begin_transaction(
            &audit_pool,
            TransactionType::Restore,
            json!({
                "action": "restore",
                "replaces_transaction": audit_id.to_string()
            }),
        )
        .await
        .map_err(|e| e.to_string())?;
        finish_transaction(&audit_pool, replacement.id, status, summary, error)
            .await
            .map_err(|e| e.to_string())?;
    }
    audit_pool.close().await;
    Ok(())
}

/// Validate a backup before replacing the live database, then close the pool
/// and copy the backup into place. The caller must restart the app afterward
/// so new connections use the restored file. The operation is recorded in
/// History, including when the backup predates the audit row.
async fn restore_backup_inner(
    pool: &Pool<Sqlite>,
    backup_path: &str,
) -> Result<RestoreOutcome, String> {
    let backup = crate::expand_user_path(backup_path.trim());
    if !backup.is_file() {
        return Err(format!("backup file does not exist: {}", backup.display()));
    }
    let options = SqliteConnectOptions::new()
        .filename(&backup)
        .read_only(true);
    let check_pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|e| format!("could not open backup: {e}"))?;
    let integrity = sqlx::query_scalar::<_, String>("PRAGMA integrity_check")
        .fetch_one(&check_pool)
        .await
        .map_err(|e| format!("backup integrity check failed: {e}"))?;
    check_pool.close().await;
    if integrity != "ok" {
        return Err(format!("backup failed integrity check: {integrity}"));
    }
    let live = database_path(pool).await?;
    let safety = live.with_extension("sqlite.before-restore");
    std::fs::copy(&live, &safety)
        .map_err(|e| format!("could not preserve current database: {e}"))?;
    pool.close().await;
    if let Err(error) = std::fs::copy(&backup, &live) {
        // A failed replacement can leave a truncated live file. Restore the
        // safety copy before surfacing the error to the user.
        let recovery = std::fs::copy(&safety, &live).err();
        return Err(match recovery {
            Some(recovery) => format!(
                "could not restore database: {error}; recovery copy also failed: {recovery}"
            ),
            None => format!("could not restore database: {error}"),
        });
    }
    Ok(RestoreOutcome {
        message: format!(
            "restored {} (pre-restore copy: {})",
            live.display(),
            safety.display()
        ),
        live_path: live,
    })
}

/// Audited public restore operation. The application must be restarted after
/// success so new connections use the restored file.
pub async fn restore_backup_core(pool: &Pool<Sqlite>, backup_path: &str) -> Result<String, String> {
    let audit = begin_transaction(
        pool,
        TransactionType::Restore,
        json!({"action":"restore", "backup_path": backup_path}),
    )
    .await
    .map_err(|e| e.to_string())?;
    match restore_backup_inner(pool, backup_path).await {
        Ok(outcome) => {
            if let Err(error) = finish_restore_audit(
                &outcome.live_path,
                audit.id,
                TransactionStatus::Succeeded,
                Some(outcome.message.clone()),
                None,
            )
            .await
            {
                return Err(format!("{}; could not record restore audit: {error}", outcome.message));
            }
            Ok(outcome.message)
        }
        Err(error) => {
            // For failures before the pool is closed this closes the running
            // row normally. If replacement failed after close, the best-effort
            // update is harmless and the pre-restore copy remains available.
            let _ = finish_transaction(
                pool,
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

async fn collect_export(pool: &Pool<Sqlite>, preferences: Value) -> Result<PortableExport, String> {
    let providers = list_providers(pool).await.map_err(|e| e.to_string())?;
    let mut endpoints = Vec::new();
    for provider in &providers {
        for mut endpoint in list_endpoints(pool, provider.id)
            .await
            .map_err(|e| e.to_string())?
        {
            endpoint.headers = redacted_map(&endpoint.headers);
            endpoints.push(endpoint);
        }
    }
    let sets = list_sets(pool).await.map_err(|e| e.to_string())?;
    let mut configuration_sets = Vec::new();
    for set in sets {
        let items = list_set_items(pool, set.id)
            .await
            .map_err(|e| e.to_string())?;
        configuration_sets.push(ConfigurationSetExport { set, items });
    }
    let mut model_routes = list_routes(pool).await.map_err(|e| e.to_string())?;
    for route in &mut model_routes {
        route.overrides = redact_value(&route.overrides);
        route.capabilities = redact_value(&route.capabilities);
    }
    let mut mcp_servers = list_mcp_servers(pool).await.map_err(|e| e.to_string())?;
    for server in &mut mcp_servers {
        server.env = redacted_map(&server.env);
        server.provenance = redact_value(&server.provenance);
    }
    let mut launch_profiles = list_profiles(pool).await.map_err(|e| e.to_string())?;
    for profile in &mut launch_profiles {
        profile.env = redacted_map(&profile.env);
        profile.native_overrides = redact_value(&profile.native_overrides);
    }
    let preferences = redact_value(&preferences);
    Ok(PortableExport {
        schema_version: 1,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        exported_at: Utc::now().to_rfc3339(),
        providers,
        endpoints,
        model_routes,
        mcp_servers,
        skills: list_skills(pool).await.map_err(|e| e.to_string())?,
        launch_profiles,
        configuration_sets,
        preferences,
    })
}

pub async fn export_config_core(pool: &Pool<Sqlite>, dest_dir: &str) -> Result<String, String> {
    export_config_core_with_preferences(pool, dest_dir, json!({})).await
}

async fn export_config_inner(
    pool: &Pool<Sqlite>,
    dest_dir: &str,
    preferences: Value,
) -> Result<String, String> {
    let export = collect_export(pool, preferences).await?;
    let destination = destination_dir(dest_dir);
    std::fs::create_dir_all(&destination).map_err(|e| e.to_string())?;
    let stamp = Utc::now().format("%Y%m%dT%H%M%S");
    let path = destination.join(format!("chm-export-{stamp}.json"));
    let bytes = serde_json::to_vec_pretty(&export).map_err(|e| e.to_string())?;
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

pub async fn backup_now_core(pool: &Pool<Sqlite>, dest_dir: &str) -> Result<String, String> {
    let audit = begin_transaction(
        pool,
        TransactionType::Manual,
        json!({"action":"backup", "destination": dest_dir}),
    )
    .await
    .map_err(|e| e.to_string())?;
    match backup_now_inner(pool, dest_dir).await {
        Ok(path) => {
            finish_transaction(
                pool,
                audit.id,
                TransactionStatus::Succeeded,
                Some(format!("database backup written to {path}")),
                None,
            )
            .await
            .map_err(|e| e.to_string())?;
            Ok(path)
        }
        Err(error) => {
            let _ = finish_transaction(
                pool,
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

pub async fn export_config_core_with_preferences(
    pool: &Pool<Sqlite>,
    dest_dir: &str,
    preferences: Value,
) -> Result<String, String> {
    let audit = begin_transaction(
        pool,
        TransactionType::Manual,
        json!({"action":"portable_export", "destination": dest_dir}),
    )
    .await
    .map_err(|e| e.to_string())?;
    match export_config_inner(pool, dest_dir, preferences).await {
        Ok(path) => {
            finish_transaction(
                pool,
                audit.id,
                TransactionStatus::Succeeded,
                Some(format!("portable configuration exported to {path}")),
                None,
            )
            .await
            .map_err(|e| e.to_string())?;
            Ok(path)
        }
        Err(error) => {
            let _ = finish_transaction(
                pool,
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

fn parse_export(path: &str) -> Result<PortableExport, String> {
    let path = crate::expand_user_path(path.trim());
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("could not read export: {e}"))?;
    let export: PortableExport =
        serde_json::from_str(&raw).map_err(|e| format!("invalid export: {e}"))?;
    if export.schema_version != 1 {
        return Err(format!(
            "unsupported export schema version {}",
            export.schema_version
        ));
    }
    Ok(export)
}

pub async fn preview_import_core(pool: &Pool<Sqlite>, file_path: &str) -> Result<Value, String> {
    let export = parse_export(file_path)?;
    let current_providers = list_providers(pool).await.map_err(|e| e.to_string())?;
    let current_mcp = list_mcp_servers(pool).await.map_err(|e| e.to_string())?;
    let current_skills = list_skills(pool).await.map_err(|e| e.to_string())?;
    let current_routes = list_routes(pool).await.map_err(|e| e.to_string())?;
    let current_profiles = list_profiles(pool).await.map_err(|e| e.to_string())?;
    let current_sets = list_sets(pool).await.map_err(|e| e.to_string())?;
    let mut current_endpoints = Vec::new();
    for provider in &current_providers {
        for endpoint in list_endpoints(pool, provider.id)
            .await
            .map_err(|e| e.to_string())?
        {
            current_endpoints.push((provider.id, endpoint));
        }
    }
    let provider_ids: HashMap<Uuid, Uuid> = export
        .providers
        .iter()
        .filter_map(|provider| {
            current_providers
                .iter()
                .find(|current| current.name.eq_ignore_ascii_case(&provider.name))
                .map(|current| (provider.id, current.id))
        })
        .collect();
    let mut additions = Vec::new();
    let mut conflicts = Vec::new();
    let mut unchanged = Vec::new();
    for provider in &export.providers {
        if let Some(current) = current_providers
            .iter()
            .find(|current| current.name.eq_ignore_ascii_case(&provider.name))
        {
            let entry = json!({"kind":"provider", "identity":provider.name});
            if provider_equivalent(provider, current) {
                unchanged.push(entry);
            } else {
                conflicts.push(json!({
                    "kind":"provider",
                    "identity":provider.name,
                    "detail":"same provider name, different settings"
                }));
            }
        } else {
            additions.push(json!({"kind":"provider", "identity":provider.name}));
        }
    }
    for endpoint in &export.endpoints {
        let identity = format!("{} · {}", endpoint.name, endpoint.base_url);
        let entry = json!({"kind":"endpoint", "identity":identity});
        let mapped_provider = provider_ids.get(&endpoint.provider_id).copied();
        let current = mapped_provider.and_then(|provider_id| {
            current_endpoints.iter().find(|(id, current)| {
                *id == provider_id
                    && normalize_url(&current.base_url) == normalize_url(&endpoint.base_url)
            })
        });
        match current {
            Some((_, current)) if endpoint_equivalent(endpoint, current) => unchanged.push(entry),
            Some(_) => conflicts.push(json!({
                "kind":"endpoint",
                "identity":identity,
                "detail":"same provider and base URL, different endpoint settings"
            })),
            None => additions.push(entry),
        }
    }
    let mut endpoint_ids: HashMap<Uuid, Uuid> = HashMap::new();
    for endpoint in &export.endpoints {
        if let Some(provider_id) = provider_ids.get(&endpoint.provider_id).copied() {
            if let Some((_, current)) = current_endpoints.iter().find(|(id, current)| {
                *id == provider_id
                    && normalize_url(&current.base_url) == normalize_url(&endpoint.base_url)
            }) {
                endpoint_ids.insert(endpoint.id, current.id);
            }
        }
    }
    let mut route_ids: HashMap<Uuid, Uuid> = HashMap::new();
    for route in &export.model_routes {
        let identity = route.remote_model_id.clone();
        let entry = json!({"kind":"model", "identity":identity});
        let current = endpoint_ids
            .get(&route.endpoint_id)
            .and_then(|endpoint_id| {
                current_routes.iter().find(|current| {
                    current.endpoint_id == *endpoint_id
                        && current
                            .remote_model_id
                            .eq_ignore_ascii_case(&route.remote_model_id)
                })
            });
        if let Some(current) = current {
            route_ids.insert(route.id, current.id);
        }
        match current {
            Some(current) if route_equivalent(route, current) => unchanged.push(entry),
            Some(_) => conflicts.push(json!({
                "kind":"model",
                "identity":identity,
                "detail":"same endpoint and remote model id, different route settings"
            })),
            None => additions.push(entry),
        }
    }
    let mut mcp_ids: HashMap<Uuid, Uuid> = HashMap::new();
    for server in &export.mcp_servers {
        let entry = json!({"kind":"mcp", "identity":server.name});
        match current_mcp
            .iter()
            .find(|current| current.name.eq_ignore_ascii_case(&server.name))
        {
            Some(current) if mcp_equivalent(server, current) => unchanged.push(entry),
            Some(_) => conflicts.push(json!({
                "kind":"mcp",
                "identity":server.name,
                "detail":"same MCP name, different settings"
            })),
            None => additions.push(entry),
        }
        if let Some(current) = current_mcp
            .iter()
            .find(|current| current.name.eq_ignore_ascii_case(&server.name))
        {
            mcp_ids.insert(server.id, current.id);
        }
    }
    let mut skill_ids: HashMap<Uuid, Uuid> = HashMap::new();
    for skill in &export.skills {
        let entry = json!({"kind":"skill", "identity":skill.canonical_path});
        match current_skills
            .iter()
            .find(|current| current.canonical_path == skill.canonical_path)
        {
            Some(current) if skill_equivalent(skill, current) => unchanged.push(entry),
            Some(_) => conflicts.push(json!({
                "kind":"skill",
                "identity":skill.canonical_path,
                "detail":"same canonical path, different metadata"
            })),
            None => additions.push(entry),
        }
        if let Some(current) = current_skills
            .iter()
            .find(|current| current.canonical_path == skill.canonical_path)
        {
            skill_ids.insert(skill.id, current.id);
        }
    }
    let mut profile_ids: HashMap<Uuid, Uuid> = HashMap::new();
    for profile in &export.launch_profiles {
        let entry = json!({"kind":"profile", "identity":profile.name});
        if let Some(current) = current_profiles
            .iter()
            .find(|current| current.name.eq_ignore_ascii_case(&profile.name))
        {
            profile_ids.insert(profile.id, current.id);
            let equivalent = current.harness_type == profile.harness_type
                && current.model_route_id
                    == profile
                        .model_route_id
                        .and_then(|id| route_ids.get(&id).copied().or(Some(id)))
                && current.provider_endpoint_id
                    == profile
                        .provider_endpoint_id
                        .and_then(|id| endpoint_ids.get(&id).copied().or(Some(id)))
                && portable_map_equivalent(&profile.env, &current.env)
                && current.role_mappings == profile.role_mappings
                && portable_value_equivalent(&profile.native_overrides, &current.native_overrides);
            if equivalent {
                unchanged.push(entry);
            } else {
                conflicts.push(json!({
                    "kind":"profile",
                    "identity":profile.name,
                    "detail":"same profile name, different launch settings"
                }));
            }
        } else {
            additions.push(entry);
        }
    }
    let current_set_items = list_set_items_for_sets(
        pool,
        &current_sets.iter().map(|set| set.id).collect::<Vec<_>>(),
    )
    .await
    .map_err(|e| e.to_string())?;
    for exported in &export.configuration_sets {
        let entry = json!({"kind":"set", "identity":exported.set.name});
        if let Some(current) = current_sets
            .iter()
            .find(|current| current.name.eq_ignore_ascii_case(&exported.set.name))
        {
            let same_description = current.description == exported.set.description;
            let same_items = current_set_items
                .get(&current.id)
                .map(|items| {
                    items
                        .iter()
                        .map(|item| (item.item_type.as_str(), item.item_id))
                        .collect::<std::collections::HashSet<_>>()
                        == exported
                            .items
                            .iter()
                            .filter_map(|item| {
                                let mapped = match item.item_type {
                                    chm_core::domain::sets::SetItemType::ModelRoute => {
                                        route_ids.get(&item.item_id)
                                    }
                                    chm_core::domain::sets::SetItemType::McpServer => {
                                        mcp_ids.get(&item.item_id)
                                    }
                                    chm_core::domain::sets::SetItemType::Skill => {
                                        skill_ids.get(&item.item_id)
                                    }
                                    chm_core::domain::sets::SetItemType::LaunchProfile => {
                                        profile_ids.get(&item.item_id)
                                    }
                                };
                                mapped.map(|id| (item.item_type.as_str(), *id))
                            })
                            .collect()
                })
                .unwrap_or(false);
            if same_description && same_items {
                unchanged.push(entry);
            } else {
                conflicts.push(json!({
                    "kind":"set",
                    "identity":exported.set.name,
                    "detail":"same set name, different description or items"
                }));
            }
        } else {
            additions.push(entry);
        }
    }
    Ok(json!({"additions": additions, "conflicts": conflicts, "unchanged": unchanged}))
}

fn merge_redacted_map(
    current: &serde_json::Map<String, Value>,
    incoming: &serde_json::Map<String, Value>,
) -> serde_json::Map<String, Value> {
    incoming
        .iter()
        .map(|(key, value)| {
            if value == &Value::String("<redacted>".into()) {
                (
                    key.clone(),
                    current.get(key).cloned().unwrap_or_else(|| value.clone()),
                )
            } else {
                (key.clone(), value.clone())
            }
        })
        .collect()
}

async fn resolve_import_credential(
    pool: &Pool<Sqlite>,
    credential: Option<&chm_core::domain::credentials::CredentialRef>,
) -> Result<Option<chm_core::domain::credentials::CredentialRef>, String> {
    let Some(credential) = credential else {
        return Ok(None);
    };
    // References are safe to export (they name an env/keychain slot, not the
    // secret value). A fresh row avoids coupling the restored database to an
    // ID from another installation.
    create_credential_ref(pool, credential.kind, &credential.reference)
        .await
        .map(Some)
        .map_err(|e| e.to_string())
}

async fn import_config_inner(
    pool: &Pool<Sqlite>,
    file_path: &str,
    mode: &str,
) -> Result<Value, String> {
    if mode != "merge" && mode != "replaceManaged" {
        return Err("import mode must be merge or replaceManaged".into());
    }
    let replace = mode == "replaceManaged";
    let export = parse_export(file_path)?;
    let mut applied = 0usize;
    let mut skipped = Vec::new();
    let mut conflicts = Vec::new();

    let mut providers = list_providers(pool).await.map_err(|e| e.to_string())?;
    let mut provider_ids: HashMap<Uuid, Uuid> = HashMap::new();
    for provider in &export.providers {
        if let Some(current) = providers
            .iter()
            .find(|current| current.name.eq_ignore_ascii_case(&provider.name))
            .cloned()
        {
            provider_ids.insert(provider.id, current.id);
            if replace && !provider_equivalent(provider, &current) {
                let changed = update_provider(
                    pool,
                    current.id,
                    &provider.display_name,
                    provider.enabled,
                    provider.notes.clone(),
                )
                .await
                .map_err(|e| e.to_string())?;
                if let Some(updated) = providers.iter_mut().find(|p| p.id == current.id) {
                    *updated = changed;
                }
                applied += 1;
            } else {
                skipped.push(format!("provider:{}", provider.name));
                if !provider_equivalent(provider, &current) {
                    conflicts.push(format!("provider:{}", provider.name));
                }
            }
        } else {
            let created = create_provider(pool, &provider.name, &provider.display_name)
                .await
                .map_err(|e| e.to_string())?;
            provider_ids.insert(provider.id, created.id);
            providers.push(created);
            applied += 1;
        }
    }

    let mut current_endpoints = Vec::new();
    for provider in &providers {
        for endpoint in list_endpoints(pool, provider.id)
            .await
            .map_err(|e| e.to_string())?
        {
            current_endpoints.push(endpoint);
        }
    }
    let mut endpoint_ids: HashMap<Uuid, Uuid> = HashMap::new();
    for endpoint in &export.endpoints {
        let Some(provider_id) = provider_ids.get(&endpoint.provider_id).copied() else {
            skipped.push(format!("endpoint:{}", endpoint.name));
            continue;
        };
        let current = current_endpoints
            .iter()
            .find(|current| {
                current.provider_id == provider_id
                    && normalize_url(&current.base_url) == normalize_url(&endpoint.base_url)
            })
            .cloned();
        if let Some(current) = current {
            endpoint_ids.insert(endpoint.id, current.id);
            let mut replacement = endpoint.clone();
            replacement.id = current.id;
            replacement.provider_id = provider_id;
            replacement.headers = merge_redacted_map(&current.headers, &endpoint.headers);
            replacement.credential_ref =
                resolve_import_credential(pool, endpoint.credential_ref.as_ref())
                    .await?
                    .or_else(|| current.credential_ref.clone());
            if replace && !endpoint_equivalent(&replacement, &current) {
                update_endpoint(pool, &replacement)
                    .await
                    .map_err(|e| e.to_string())?;
                if let Some(existing) = current_endpoints.iter_mut().find(|e| e.id == current.id) {
                    *existing = replacement;
                }
                applied += 1;
            } else {
                skipped.push(format!("endpoint:{}", endpoint.name));
                if !endpoint_equivalent(&replacement, &current) {
                    conflicts.push(format!("endpoint:{}", endpoint.name));
                }
            }
        } else {
            let mut copy = endpoint.clone();
            copy.id = Uuid::new_v4();
            copy.provider_id = provider_id;
            copy.credential_ref =
                resolve_import_credential(pool, endpoint.credential_ref.as_ref()).await?;
            create_endpoint(pool, &copy)
                .await
                .map_err(|e| e.to_string())?;
            endpoint_ids.insert(endpoint.id, copy.id);
            current_endpoints.push(copy);
            applied += 1;
        }
    }

    let mut current_routes = list_routes(pool).await.map_err(|e| e.to_string())?;
    let mut route_ids: HashMap<Uuid, Uuid> = HashMap::new();
    for route in &export.model_routes {
        let Some(endpoint_id) = endpoint_ids.get(&route.endpoint_id).copied() else {
            skipped.push(format!("model:{}", route.remote_model_id));
            continue;
        };
        let current = current_routes
            .iter()
            .find(|current| {
                current.endpoint_id == endpoint_id
                    && current
                        .remote_model_id
                        .eq_ignore_ascii_case(&route.remote_model_id)
            })
            .cloned();
        if let Some(current) = current {
            route_ids.insert(route.id, current.id);
            let mut replacement = route.clone();
            replacement.id = current.id;
            replacement.endpoint_id = endpoint_id;
            replacement.model_identity_id = None;
            if replace && !route_equivalent(&replacement, &current) {
                update_route(pool, &replacement)
                    .await
                    .map_err(|e| e.to_string())?;
                if let Some(existing) = current_routes.iter_mut().find(|r| r.id == current.id) {
                    *existing = replacement;
                }
                applied += 1;
            } else {
                skipped.push(format!("model:{}", route.remote_model_id));
                if !route_equivalent(&replacement, &current) {
                    conflicts.push(format!("model:{}", route.remote_model_id));
                }
            }
        } else {
            let mut copy = route.clone();
            copy.id = Uuid::new_v4();
            copy.endpoint_id = endpoint_id;
            copy.model_identity_id = None;
            create_route(pool, &copy).await.map_err(|e| e.to_string())?;
            route_ids.insert(route.id, copy.id);
            current_routes.push(copy);
            applied += 1;
        }
    }

    let mut current_mcp = list_mcp_servers(pool).await.map_err(|e| e.to_string())?;
    let mut mcp_ids: HashMap<Uuid, Uuid> = HashMap::new();
    for server in &export.mcp_servers {
        if let Some(current) = current_mcp
            .iter()
            .find(|current| current.name.eq_ignore_ascii_case(&server.name))
            .cloned()
        {
            mcp_ids.insert(server.id, current.id);
            let mut replacement = server.clone();
            replacement.id = current.id;
            replacement.env = merge_redacted_map(&current.env, &server.env);
            if replace && !mcp_equivalent(&replacement, &current) {
                update_mcp_server(pool, &replacement)
                    .await
                    .map_err(|e| e.to_string())?;
                if let Some(existing) = current_mcp.iter_mut().find(|s| s.id == current.id) {
                    *existing = replacement;
                }
                applied += 1;
            } else {
                skipped.push(format!("mcp:{}", server.name));
                if !mcp_equivalent(&replacement, &current) {
                    conflicts.push(format!("mcp:{}", server.name));
                }
            }
        } else {
            let mut copy = server.clone();
            copy.id = Uuid::new_v4();
            create_mcp_server(pool, &copy)
                .await
                .map_err(|e| e.to_string())?;
            mcp_ids.insert(server.id, copy.id);
            current_mcp.push(copy);
            applied += 1;
        }
    }

    let mut current_skills = list_skills(pool).await.map_err(|e| e.to_string())?;
    let mut skill_ids: HashMap<Uuid, Uuid> = HashMap::new();
    for skill in &export.skills {
        if let Some(current) = current_skills
            .iter()
            .find(|current| current.canonical_path == skill.canonical_path)
            .cloned()
        {
            skill_ids.insert(skill.id, current.id);
            let mut replacement = skill.clone();
            replacement.id = current.id;
            if replace && !skill_equivalent(&replacement, &current) {
                update_skill(pool, &replacement)
                    .await
                    .map_err(|e| e.to_string())?;
                if let Some(existing) = current_skills.iter_mut().find(|s| s.id == current.id) {
                    *existing = replacement;
                }
                applied += 1;
            } else {
                skipped.push(format!("skill:{}", skill.name));
                if !skill_equivalent(&replacement, &current) {
                    conflicts.push(format!("skill:{}", skill.name));
                }
            }
        } else {
            let mut copy = skill.clone();
            copy.id = Uuid::new_v4();
            create_skill(pool, &copy).await.map_err(|e| e.to_string())?;
            skill_ids.insert(skill.id, copy.id);
            current_skills.push(copy);
            applied += 1;
        }
    }

    let mut current_profiles = list_profiles(pool).await.map_err(|e| e.to_string())?;
    let mut profile_ids: HashMap<Uuid, Uuid> = HashMap::new();
    for profile in &export.launch_profiles {
        let mut replacement = profile.clone();
        replacement.model_route_id = profile
            .model_route_id
            .and_then(|id| route_ids.get(&id).copied());
        replacement.provider_endpoint_id = profile
            .provider_endpoint_id
            .and_then(|id| endpoint_ids.get(&id).copied());
        if let Some(current) = current_profiles
            .iter()
            .find(|current| current.name.eq_ignore_ascii_case(&profile.name))
            .cloned()
        {
            profile_ids.insert(profile.id, current.id);
            replacement.id = current.id;
            replacement.env = merge_redacted_map(&current.env, &profile.env);
            let current_overrides = current
                .native_overrides
                .as_object()
                .cloned()
                .unwrap_or_default();
            let imported_overrides = profile
                .native_overrides
                .as_object()
                .cloned()
                .unwrap_or_default();
            replacement.native_overrides =
                Value::Object(merge_redacted_map(&current_overrides, &imported_overrides));
            if replace
                && (replacement.harness_type != current.harness_type
                    || replacement.model_route_id != current.model_route_id
                    || replacement.provider_endpoint_id != current.provider_endpoint_id
                    || replacement.env != current.env
                    || replacement.role_mappings != current.role_mappings
                    || replacement.native_overrides != current.native_overrides)
            {
                update_profile(pool, &replacement)
                    .await
                    .map_err(|e| e.to_string())?;
                if let Some(existing) = current_profiles.iter_mut().find(|p| p.id == current.id) {
                    *existing = replacement;
                }
                applied += 1;
            } else {
                skipped.push(format!("profile:{}", profile.name));
                if replacement.harness_type != current.harness_type
                    || replacement.model_route_id != current.model_route_id
                    || replacement.provider_endpoint_id != current.provider_endpoint_id
                {
                    conflicts.push(format!("profile:{}", profile.name));
                }
            }
        } else {
            replacement.id = Uuid::new_v4();
            create_profile(pool, &replacement)
                .await
                .map_err(|e| e.to_string())?;
            profile_ids.insert(profile.id, replacement.id);
            current_profiles.push(replacement);
            applied += 1;
        }
    }

    for exported in &export.configuration_sets {
        let existing = list_sets(pool).await.map_err(|e| e.to_string())?;
        let current = existing
            .iter()
            .find(|current| current.name.eq_ignore_ascii_case(&exported.set.name))
            .cloned();
        let set = if let Some(mut current) = current {
            if !replace {
                skipped.push(format!("set:{}", exported.set.name));
                continue;
            }
            current.description = exported.set.description.clone();
            current.updated_at = Utc::now();
            update_set(pool, &current)
                .await
                .map_err(|e| e.to_string())?;
            clear_set_items(pool, current.id)
                .await
                .map_err(|e| e.to_string())?;
            applied += 1;
            current
        } else {
            let created = create_set(pool, &exported.set.name, exported.set.description.clone())
                .await
                .map_err(|e| e.to_string())?;
            applied += 1;
            created
        };
        for item in &exported.items {
            let mapped = match item.item_type {
                chm_core::domain::sets::SetItemType::ModelRoute => route_ids.get(&item.item_id),
                chm_core::domain::sets::SetItemType::McpServer => mcp_ids.get(&item.item_id),
                chm_core::domain::sets::SetItemType::Skill => skill_ids.get(&item.item_id),
                chm_core::domain::sets::SetItemType::LaunchProfile => {
                    profile_ids.get(&item.item_id)
                }
            };
            if let Some(item_id) = mapped {
                add_set_item(pool, set.id, item.item_type, *item_id)
                    .await
                    .map_err(|e| e.to_string())?;
            } else {
                skipped.push(format!(
                    "set-item:{}:{:?}",
                    exported.set.name, item.item_type
                ));
            }
        }
    }
    Ok(json!({
        "applied": applied,
        "skipped": skipped,
        "conflicts": conflicts,
        "mode": mode
    }))
}

/// Apply a portable import and record it in History even though it changes
/// registry rows rather than harness files. The audit row makes recovery
/// operations visible alongside native sync/manual edits.
pub async fn import_config_core(
    pool: &Pool<Sqlite>,
    file_path: &str,
    mode: &str,
) -> Result<Value, String> {
    let audit = begin_transaction(
        pool,
        TransactionType::Import,
        json!({"file_path": file_path, "mode": mode}),
    )
    .await
    .map_err(|e| e.to_string())?;
    match import_config_inner(pool, file_path, mode).await {
        Ok(result) => {
            let applied = result
                .get("applied")
                .and_then(Value::as_u64)
                .unwrap_or_default();
            finish_transaction(
                pool,
                audit.id,
                TransactionStatus::Succeeded,
                Some(format!("portable import applied {applied} item(s)")),
                None,
            )
            .await
            .map_err(|e| e.to_string())?;
            Ok(result)
        }
        Err(error) => {
            let _ = finish_transaction(
                pool,
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

#[tauri::command]
pub async fn backup_now_cmd(
    state: State<'_, AppState>,
    dest_dir: String,
) -> Result<String, String> {
    backup_now_core(&state.pool, &dest_dir).await
}

#[tauri::command]
pub async fn list_backups_cmd(dest_dir: String) -> Result<Vec<String>, String> {
    list_backups_core(&dest_dir)
}

#[tauri::command]
pub async fn restore_backup_cmd(
    state: State<'_, AppState>,
    backup_path: String,
) -> Result<String, String> {
    restore_backup_core(&state.pool, &backup_path).await
}

#[tauri::command]
pub async fn export_config_cmd(
    state: State<'_, AppState>,
    dest_dir: String,
    preferences: Option<Value>,
) -> Result<String, String> {
    export_config_core_with_preferences(
        &state.pool,
        &dest_dir,
        preferences.unwrap_or_else(|| json!({})),
    )
    .await
}

#[tauri::command]
pub async fn preview_import_cmd(
    state: State<'_, AppState>,
    file_path: String,
) -> Result<Value, String> {
    preview_import_core(&state.pool, &file_path).await
}

#[tauri::command]
pub async fn import_config_cmd(
    state: State<'_, AppState>,
    file_path: String,
    mode: String,
) -> Result<Value, String> {
    import_config_core(&state.pool, &file_path, &mode).await
}

#[cfg(test)]
mod tests {
    use super::{
        backup_now_core, export_config_core, import_config_core, preview_import_core,
        restore_backup_core,
    };
    use chm_core::domain::mcp::{McpServer, McpTransport, ScopeType};
    use chm_core::domain::models::ModelRoute;
    use chm_core::domain::profiles::{LaunchProfile, RoleMapping};
    use chm_core::domain::provider::{AuthType, Protocol, ProviderEndpoint};
    use chm_core::domain::sets::SetItemType;
    use chm_database::connect_test;
    use chm_database::repos::mcp::create_mcp_server;
    use chm_database::repos::models::create_route;
    use chm_database::repos::profiles::{
        add_set_item, create_profile, create_set, list_profiles, list_sets,
    };
    use chm_database::repos::history::list_transactions;
    use chm_database::repos::providers::{
        create_endpoint, create_provider, list_providers, update_provider,
    };
    use tempfile::TempDir;
    use uuid::Uuid;

    #[tokio::test]
    async fn export_roundtrip_is_credential_safe_and_idempotent() {
        let pool = connect_test().await.unwrap();
        let provider = create_provider(&pool, "demo", "Demo").await.unwrap();
        let endpoint = ProviderEndpoint {
            id: Uuid::new_v4(),
            provider_id: provider.id,
            name: "API".into(),
            base_url: "https://example.test/v1".into(),
            protocol: Protocol::OpenAiChatCompletions,
            discovery_path: Some("/models".into()),
            auth_type: AuthType::BearerToken,
            credential_ref: None,
            headers: Default::default(),
            enabled: true,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        create_endpoint(&pool, &endpoint).await.unwrap();
        let dir = TempDir::new().unwrap();
        let path = export_config_core(&pool, &dir.path().display().to_string())
            .await
            .unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("example.test"));
        assert!(!raw.contains("secret-value"));

        let fresh = connect_test().await.unwrap();
        let preview = preview_import_core(&fresh, &path).await.unwrap();
        assert_eq!(preview["additions"].as_array().unwrap().len(), 2);
        let first = import_config_core(&fresh, &path, "merge").await.unwrap();
        assert_eq!(first["applied"], 2);
        let second = import_config_core(&fresh, &path, "merge").await.unwrap();
        assert_eq!(second["applied"], 0);
    }

    #[tokio::test]
    async fn portable_export_redacts_secrets_and_roundtrips_profiles_and_sets() {
        let pool = connect_test().await.unwrap();
        let provider = create_provider(&pool, "demo", "Demo").await.unwrap();
        let mut headers = serde_json::Map::new();
        headers.insert("Authorization".into(), "Bearer super-secret".into());
        headers.insert("X-Region".into(), "test".into());
        let endpoint = ProviderEndpoint {
            id: Uuid::new_v4(),
            provider_id: provider.id,
            name: "API".into(),
            base_url: "https://example.test/v1".into(),
            protocol: Protocol::OpenAiChatCompletions,
            discovery_path: Some("/models".into()),
            auth_type: AuthType::BearerToken,
            credential_ref: None,
            headers,
            enabled: true,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        create_endpoint(&pool, &endpoint).await.unwrap();
        let mut route = ModelRoute::new(
            "demo-model".into(),
            "Demo model".into(),
            Some(32_000),
            serde_json::json!({}),
            serde_json::json!({}),
        );
        route.endpoint_id = endpoint.id;
        create_route(&pool, &route).await.unwrap();
        let profile = LaunchProfile {
            id: Uuid::new_v4(),
            name: "Demo profile".into(),
            harness_type: chm_core::domain::harness::HarnessType::Pi,
            model_route_id: Some(route.id),
            provider_endpoint_id: Some(endpoint.id),
            env: serde_json::json!({"apiToken":"profile-secret","safe":"ok"})
                .as_object()
                .unwrap()
                .clone(),
            role_mappings: vec![RoleMapping {
                role: "default".into(),
                model: "demo-model".into(),
            }],
            native_overrides: serde_json::json!({}),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        create_profile(&pool, &profile).await.unwrap();
        let set = create_set(&pool, "Demo set", Some("portable".into()))
            .await
            .unwrap();
        add_set_item(&pool, set.id, SetItemType::ModelRoute, route.id)
            .await
            .unwrap();
        let mcp = McpServer {
            id: Uuid::new_v4(),
            name: "Demo MCP".into(),
            transport: McpTransport::Stdio,
            command: Some("demo-mcp".into()),
            args: vec![],
            url: None,
            env: serde_json::json!({"token":"mcp-secret"})
                .as_object()
                .unwrap()
                .clone(),
            scope_type: ScopeType::Global,
            scope_path: None,
            provenance: serde_json::json!({}),
            enabled: true,
        };
        create_mcp_server(&pool, &mcp).await.unwrap();

        let dir = TempDir::new().unwrap();
        let path = export_config_core(&pool, &dir.path().display().to_string())
            .await
            .unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("super-secret"));
        assert!(!raw.contains("profile-secret"));
        assert!(!raw.contains("mcp-secret"));
        assert!(raw.contains("<redacted>"));

        let fresh = connect_test().await.unwrap();
        let preview = preview_import_core(&fresh, &path).await.unwrap();
        assert!(preview["additions"].as_array().unwrap().len() >= 5);
        let imported = import_config_core(&fresh, &path, "merge").await.unwrap();
        assert!(imported["applied"].as_u64().unwrap() >= 5);
        assert_eq!(list_profiles(&fresh).await.unwrap().len(), 1);
        assert_eq!(list_sets(&fresh).await.unwrap().len(), 1);
        // Redacted secret markers are wildcards during comparison, so a
        // second preview is clean instead of reporting the preserved secret
        // fields as conflicts.
        let repeat_preview = preview_import_core(&fresh, &path).await.unwrap();
        assert!(repeat_preview["conflicts"].as_array().unwrap().is_empty());

        // A changed existing provider is previewed as a conflict. Merge
        // leaves it intact; replace-managed updates it.
        let provider_id = chm_database::repos::providers::list_providers(&fresh)
            .await
            .unwrap()[0]
            .id;
        update_provider(&fresh, provider_id, "Changed", true, None)
            .await
            .unwrap();
        let conflict = preview_import_core(&fresh, &path).await.unwrap();
        assert!(
            conflict["conflicts"]
                .as_array()
                .unwrap()
                .iter()
                .any(|entry| entry["kind"] == "provider")
        );
        let merge = import_config_core(&fresh, &path, "merge").await.unwrap();
        assert!(merge["conflicts"].as_array().unwrap().iter().any(|entry| {
            entry
                .as_str()
                .is_some_and(|value| value.starts_with("provider:"))
        }));
        let replaced = import_config_core(&fresh, &path, "replaceManaged")
            .await
            .unwrap();
        assert!(replaced["applied"].as_u64().unwrap() >= 1);
    }

    #[tokio::test]
    async fn restore_replaces_database_and_records_audit() {
        let dir = TempDir::new().unwrap();
        let live_path = dir.path().join("live.sqlite");
        let pool = chm_database::connect(live_path.to_str().unwrap())
            .await
            .unwrap();
        let provider = create_provider(&pool, "demo", "Before").await.unwrap();
        let backup_dir = dir.path().join("backups");
        let backup = backup_now_core(&pool, &backup_dir.display().to_string())
            .await
            .unwrap();
        update_provider(&pool, provider.id, "After", true, None)
            .await
            .unwrap();

        let message = restore_backup_core(&pool, &backup).await.unwrap();
        assert!(message.contains("pre-restore copy"));

        let restored = chm_database::connect(live_path.to_str().unwrap())
            .await
            .unwrap();
        let providers = list_providers(&restored).await.unwrap();
        assert_eq!(providers[0].display_name, "Before");
        assert!(list_transactions(&restored)
            .await
            .unwrap()
            .iter()
            .any(|tx| {
                tx.transaction_type == chm_core::domain::history::TransactionType::Restore
                    && tx.status == chm_core::domain::history::TransactionStatus::Succeeded
            }));
    }
}
