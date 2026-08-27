//! Claude Code native config parser (settings.json + ~/.claude.json mcpServers).
//! Shape verified on 2.1.246 — see docs/harnesses/claude-code.md.

use chm_core::domain::mcp::{McpServer, McpTransport, ScopeType};
use chm_core::domain::models::ModelRoute;
use chm_harness_sdk::adapter::types::{AdapterError, HarnessMcp, HarnessModel, ParsedState};
use chrono::Utc;
use uuid::Uuid;

const ROLE_ENV_VARS: &[(&str, &str)] = &[
    ("ANTHROPIC_DEFAULT_OPUS_MODEL", "opus"),
    ("ANTHROPIC_DEFAULT_SONNET_MODEL", "sonnet"),
    ("ANTHROPIC_DEFAULT_HAIKU_MODEL", "haiku"),
];

pub fn parse_config(
    settings_raw: Option<&str>,
    claude_json_raw: Option<&str>,
    home: &std::path::Path,
) -> Result<ParsedState, AdapterError> {
    let mut state = ParsedState::default();

    // settings.json: env block (role mappings + provider overrides)
    if let Some(raw) = settings_raw {
        let json: serde_json::Value =
            serde_json::from_str(raw).map_err(|e| AdapterError::Parse {
                path: "settings.json".into(),
                detail: e.to_string(),
            })?;
        if let Some(env) = json.get("env").and_then(|e| e.as_object()) {
            for (key, value) in env {
                if let Some((_, role)) = ROLE_ENV_VARS.iter().find(|(k, _)| *k == key) {
                    if let Some(model) = value.as_str() {
                        let route = ModelRoute {
                            id: Uuid::new_v4(),
                            endpoint_id: Uuid::new_v4(),
                            model_identity_id: None,
                            remote_model_id: model.to_string(),
                            display_name: format!("{role} role"),
                            context_window: None,
                            max_input: None,
                            max_output: None,
                            capabilities: serde_json::json!({"role": role}),
                            overrides: serde_json::json!({"env_key": key}),
                            enabled: true,
                            created_at: Utc::now(),
                            updated_at: Utc::now(),
                        };
                        state.models.push(HarnessModel {
                            native_id: role.to_string(),
                            route,
                        });
                    }
                } else {
                    state.providers.push(serde_json::json!({
                        "env_override": key,
                        "value": value,
                        "source": "settings.json",
                    }));
                }
            }
        }
    }

    // ~/.claude.json: global mcpServers
    if let Some(raw) = claude_json_raw {
        let json: serde_json::Value =
            serde_json::from_str(raw).map_err(|e| AdapterError::Parse {
                path: "~/.claude.json".into(),
                detail: e.to_string(),
            })?;
        if let Some(mcp) = json.get("mcpServers").and_then(|m| m.as_object()) {
            for (name, spec) in mcp {
                state.mcp.push(HarnessMcp {
                    native_name: name.clone(),
                    server: parse_mcp(name, spec),
                });
            }
        }
    }

    // skills: ~/.claude/skills
    let skills_dir = home.join(".claude/skills");
    if skills_dir.is_dir() {
        for entry in std::fs::read_dir(&skills_dir)? {
            let entry = entry?;
            if entry.path().is_dir() {
                state
                    .skills
                    .push(chm_harness_sdk::adapter::types::HarnessSkill {
                        name: entry.file_name().to_string_lossy().into_owned(),
                        path: entry.path().display().to_string(),
                        content_hash: None,
                        symlinked: entry
                            .path()
                            .symlink_metadata()
                            .map(|m| m.file_type().is_symlink())
                            .unwrap_or(false),
                    });
            }
        }
    }

    Ok(state)
}

fn parse_mcp(name: &str, spec: &serde_json::Value) -> McpServer {
    let transport = match spec.get("type").and_then(|t| t.as_str()) {
        Some("http") | Some("sse") => McpTransport::Http,
        _ => McpTransport::Stdio,
    };
    let mut env: serde_json::Map<String, serde_json::Value> = spec
        .get("env")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    if let Some(headers) = spec.get("headers").and_then(|v| v.as_object()) {
        env.insert("headers".into(), serde_json::Value::Object(headers.clone()));
    }
    McpServer {
        id: Uuid::new_v4(),
        name: name.to_string(),
        transport,
        command: spec
            .get("command")
            .and_then(|v| v.as_str())
            .map(String::from),
        args: spec
            .get("args")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        url: spec.get("url").and_then(|v| v.as_str()).map(String::from),
        env,
        scope_type: ScopeType::Global,
        scope_path: None,
        provenance: serde_json::json!({"source": "claude-json-mcpServers"}),
        enabled: true,
    }
}
