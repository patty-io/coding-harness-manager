//! MCP registry commands + bindings + diagnostics.

use chm_core::domain::harness::HarnessMcpBinding;
use chm_core::domain::mcp::{McpServer, McpTransport, ScopeType};
use chm_database::repos::harness::list_installations;
use chm_database::repos::mcp::{
    create_mcp_binding, create_mcp_server, list_mcp_bindings, list_mcp_servers,
};
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Sqlite};
use tauri::State;
use uuid::Uuid;

use crate::AppState;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpInput {
    pub name: String,
    pub transport: String,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
    pub env: serde_json::Map<String, serde_json::Value>,
}

#[tauri::command]
pub async fn create_mcp_cmd(
    state: State<'_, AppState>,
    input: McpInput,
) -> Result<McpServer, String> {
    if input.name.trim().is_empty() {
        return Err("name is required".into());
    }
    let server = McpServer {
        id: Uuid::new_v4(),
        name: input.name,
        transport: McpTransport::parse_str(&input.transport),
        command: input.command,
        args: input.args,
        url: input.url,
        env: input.env,
        scope_type: ScopeType::Global,
        scope_path: None,
        provenance: serde_json::json!({"source": "manual"}),
        enabled: true,
    };
    create_mcp_server(&state.pool, &server)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_mcp_cmd(state: State<'_, AppState>) -> Result<Vec<McpServer>, String> {
    list_mcp_servers(&state.pool)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_mcp_cmd(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let id = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    chm_database::repos::mcp::delete_mcp_server(&state.pool, id)
        .await
        .map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BindingView {
    pub installation_id: String,
    pub harness_type: String,
    pub native_name: String,
    pub managed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpDetail {
    pub server: McpServer,
    pub bindings: Vec<BindingView>,
}

#[tauri::command]
pub async fn mcp_detail_cmd(
    state: State<'_, AppState>,
    mcp_id: String,
) -> Result<McpDetail, String> {
    let id = Uuid::parse_str(&mcp_id).map_err(|e| e.to_string())?;
    let server = list_mcp_servers(&state.pool)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|s| s.id == id)
        .ok_or("mcp server not found")?;
    let installs = list_installations(&state.pool)
        .await
        .map_err(|e| e.to_string())?;
    let mut bindings = Vec::new();
    for inst in &installs {
        for b in list_mcp_bindings(&state.pool, inst.id)
            .await
            .map_err(|e| e.to_string())?
        {
            if b.mcp_server_id == id {
                bindings.push(BindingView {
                    installation_id: inst.id.to_string(),
                    harness_type: inst.harness_type.as_str().to_string(),
                    native_name: b.native_name.clone(),
                    managed: b.managed,
                });
            }
        }
    }
    Ok(McpDetail { server, bindings })
}

/// Binds a canonical server to a harness by creating the binding row AND
/// syncing the MCP entry into the native config (via the sync engine's MCP arm).
#[tauri::command]
pub async fn bind_mcp_cmd(
    state: State<'_, AppState>,
    installation_id: String,
    mcp_id: String,
) -> Result<(), String> {
    let iid = Uuid::parse_str(&installation_id).map_err(|e| e.to_string())?;
    let mid = Uuid::parse_str(&mcp_id).map_err(|e| e.to_string())?;
    let inst = list_installations(&state.pool)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|i| i.id == iid)
        .ok_or("installation not found")?;
    let server = list_mcp_servers(&state.pool)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|s| s.id == mid)
        .ok_or("mcp server not found")?;

    // write into the harness config through the sync engine (append, mcp only)
    crate::commands::sync::bind_mcp_sync(&state.pool, &inst, &server).await?;

    create_mcp_binding(
        &state.pool,
        &HarnessMcpBinding {
            id: Uuid::new_v4(),
            harness_installation_id: iid,
            mcp_server_id: mid,
            native_name: server.name.clone(),
            native_config: serde_json::json!({"command": server.command, "args": server.args}),
            managed: true,
        },
    )
    .await
    .map_err(|e| e.to_string())
}

// --- diagnostics ---

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    pub check: String,
    pub passed: bool,
    pub detail: String,
}

pub async fn run_mcp_diagnostics_core(
    pool: &Pool<Sqlite>,
    mcp_id: &str,
) -> Result<Vec<CheckResult>, String> {
    let id = Uuid::parse_str(mcp_id).map_err(|e| e.to_string())?;
    let server = list_mcp_servers(pool)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|s| s.id == id)
        .ok_or("mcp server not found")?;
    let mut checks = Vec::new();

    // 1. command exists
    let command_exists = match &server.command {
        Some(cmd) => {
            let found = crate::commands::scan::command_on_path(cmd);
            checks.push(CheckResult {
                check: "command exists".into(),
                passed: found,
                detail: if found {
                    format!("{cmd} found on PATH")
                } else {
                    format!("{cmd} not found on PATH")
                },
            });
            found
        }
        None => {
            checks.push(CheckResult {
                check: "command exists".into(),
                passed: true,
                detail: "http/sse transport — no command needed".into(),
            });
            true
        }
    };

    // 2. env available
    let env_ok = server.env.iter().all(|(k, v)| match v.as_str() {
        Some(val) if val.starts_with("$LP_") => {
            let name = val.trim_start_matches("$LP_");
            std::env::var_os(name).is_some()
        }
        _ => k != "headers" && k != "_direct_tools",
    });
    checks.push(CheckResult {
        check: "env available".into(),
        passed: env_ok,
        detail: if env_ok {
            "all env references resolve".into()
        } else {
            "some env references are unresolved".into()
        },
    });

    // 3. executable launches (stdio only) — bounded spawn probe
    if command_exists && matches!(server.transport, McpTransport::Stdio) {
        let cmd = server.command.clone().unwrap_or_default();
        let mut spawn_cmd = tokio::process::Command::new(&cmd);
        if !server.args.is_empty() {
            spawn_cmd.arg(&server.args[0]);
        }
        spawn_cmd
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        let spawned = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            match spawn_cmd.spawn() {
                Ok(mut child) => {
                    let _ = child.start_kill();
                    let _ = child.wait().await; // reap — no zombie
                    true
                }
                Err(_) => false,
            }
        })
        .await
        .unwrap_or(false);
        checks.push(CheckResult {
            check: "executable launches".into(),
            passed: spawned,
            detail: if spawned {
                format!("{cmd} spawns successfully")
            } else {
                format!("failed to spawn {cmd} (or timed out)")
            },
        });
    }

    // 4. http reachable (http/sse)
    if let Some(url) = &server.url {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .build()
            .map_err(|e| e.to_string())?;
        let ok = http.get(url).send().await.is_ok();
        checks.push(CheckResult {
            check: "http reachable".into(),
            passed: ok,
            detail: if ok {
                format!("{url} answered")
            } else {
                format!("{url} unreachable")
            },
        });
    } else {
        checks.push(CheckResult {
            check: "http reachable".into(),
            passed: true,
            detail: "stdio transport".into(),
        });
    }

    Ok(checks)
}

#[tauri::command]
pub async fn run_mcp_diagnostics(
    state: State<'_, AppState>,
    mcp_id: String,
) -> Result<Vec<CheckResult>, String> {
    run_mcp_diagnostics_core(&state.pool, &mcp_id).await
}
