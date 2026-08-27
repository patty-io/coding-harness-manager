//! Claude Code native config parser (settings.json + ~/.claude.json mcpServers).
//! Shape verified on 2.1.246 — see docs/harnesses/claude-code.md.

use chm_core::domain::mcp::McpServer;
use chm_core::domain::models::ModelRoute;
use chm_harness_sdk::adapter::types::{AdapterError, HarnessMcp, HarnessModel, ParsedState};

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
                        let route = ModelRoute::new(
                            model.to_string(),
                            format!("{role} role"),
                            None,
                            serde_json::json!({"role": role}),
                            serde_json::json!({"env_key": key}),
                        );
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
    state
        .skills
        .extend(chm_harness_sdk::adapter::helpers::scan_skills_dir(
            &home.join(".claude/skills"),
        ));

    Ok(state)
}
fn parse_mcp(name: &str, spec: &serde_json::Value) -> McpServer {
    chm_harness_sdk::adapter::helpers::parse_mcp_json(
        name,
        spec,
        serde_json::json!({"source": "claude-json-mcpServers"}),
    )
}
