//! Codex native config parser.
//! Shape verified on 0.150.0 — modern per-provider config files plus legacy
//! model_providers/models in the main config.toml. See docs/harnesses/codex.md.

use chm_core::domain::mcp::{McpServer, McpTransport, ScopeType};
use chm_core::domain::models::ModelRoute;
use chm_harness_sdk::adapter::types::{AdapterError, HarnessMcp, HarnessModel, ParsedState};
use chrono::Utc;
use uuid::Uuid;

/// Parses the main config.toml (modern selection + mcp_servers, legacy providers/models).
pub fn parse_main_config(raw: &str, home: &std::path::Path) -> Result<ParsedState, AdapterError> {
    let toml: toml::Value = toml::from_str(raw).map_err(|e| AdapterError::Parse {
        path: "~/.codex/config.toml".into(),
        detail: e.to_string(),
    })?;
    let mut state = ParsedState::default();

    // legacy layout: [model_providers.<id>] + [models.<id>] inside the main file
    if let Some(mps) = toml.get("model_providers").and_then(|m| m.as_table()) {
        for (pid, pv) in mps {
            state.providers.push(serde_json::json!({
                "native_provider_id": pid,
                "base_url": pv.get("base_url"),
                "env_key": pv.get("env_key"),
                "wire_api": pv.get("wire_api"),
                "layout": "legacy-model_providers",
            }));
        }
        if let Some(models) = toml.get("models").and_then(|m| m.as_table()) {
            for (mid, mv) in models {
                let provider_id = mv
                    .get("provider")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let wire_api = toml
                    .get("model_providers")
                    .and_then(|mps| mps.get(provider_id))
                    .and_then(|pv| pv.get("wire_api"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("chat");
                push_model(&mut state, mid, mv, provider_id, wire_api);
            }
        }
    } else if let Some(ps) = toml.get("providers").and_then(|m| m.as_table()) {
        // older legacy layout [providers.<id>]
        for (pid, pv) in ps {
            state.providers.push(serde_json::json!({
                "native_provider_id": pid,
                "base_url": pv.get("base_url"),
                "env_key": pv.get("env_key"),
                "wire_api": pv.get("wire_api"),
                "layout": "legacy-providers",
            }));
        }
        state
            .warnings
            .push("legacy [providers] layout detected in config.toml".into());
    }

    // modern: [mcp_servers.<name>] in the main file
    if let Some(mcps) = toml.get("mcp_servers").and_then(|m| m.as_table()) {
        for (name, spec) in mcps {
            state.mcp.push(HarnessMcp {
                native_name: name.clone(),
                server: parse_mcp(name, spec),
            });
        }
    }

    // selection keys
    state.profiles.push(serde_json::json!({
        "default_model": toml.get("model"),
        "model_reasoning_effort": toml.get("model_reasoning_effort"),
    }));

    // modern per-provider files: ~/.codex/<id>.config.toml
    let codex_dir = home.join(".codex");
    if codex_dir.is_dir() {
        for entry in std::fs::read_dir(&codex_dir)? {
            let entry = entry?;
            let fname = entry.file_name().to_string_lossy().into_owned();
            if fname.ends_with(".config.toml") {
                let id = fname.trim_end_matches(".config.toml").to_string();
                let raw = std::fs::read_to_string(entry.path())?;
                parse_provider_file(&raw, &id, &mut state);
            }
        }
    }

    // skills: ~/.codex/skills
    let skills_dir = home.join(".codex/skills");
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

/// Modern per-provider config file: ~/.codex/<id>.config.toml
/// (top-level model selection + [model_providers.<id>] + optional [mcp_servers]).
fn parse_provider_file(raw: &str, file_id: &str, state: &mut ParsedState) {
    let Ok(toml) = raw.parse::<toml::Value>() else {
        state
            .warnings
            .push(format!("{file_id}.config.toml is not valid TOML"));
        return;
    };
    let provider_id = toml
        .get("model_provider")
        .and_then(|v| v.as_str())
        .unwrap_or(file_id);
    if let Some(pv) = toml.get("model_providers").and_then(|m| m.get(provider_id)) {
        state.providers.push(serde_json::json!({
            "native_provider_id": provider_id,
            "base_url": pv.get("base_url"),
            "env_key": pv.get("env_key"),
            "wire_api": pv.get("wire_api"),
            "layout": "modern-provider-file",
            "file": format!("{file_id}.config.toml"),
        }));
    }
    if let Some(model_id) = toml.get("model").and_then(|v| v.as_str()) {
        let wire_api = toml
            .get("model_providers")
            .and_then(|m| m.get(provider_id))
            .and_then(|pv| pv.get("wire_api"))
            .and_then(|v| v.as_str())
            .unwrap_or("chat");
        // strip provider prefix from the selected model id ("zai/glm-5.2" -> "glm-5.2")
        let remote_id = model_id
            .split('/')
            .next_back()
            .unwrap_or(model_id)
            .to_string();
        let route = ModelRoute {
            id: Uuid::new_v4(),
            endpoint_id: Uuid::new_v4(),
            model_identity_id: None,
            remote_model_id: remote_id,
            display_name: model_id.to_string(),
            context_window: toml
                .get("model_context_window")
                .and_then(|v| v.as_integer()),
            max_input: None,
            max_output: None,
            capabilities: serde_json::json!({
                "model": model_id,
                "reasoning_effort": toml.get("model_reasoning_effort"),
                "supports_reasoning_summaries": toml.get("model_supports_reasoning_summaries"),
                "auto_compact_token_limit": toml.get("model_auto_compact_token_limit"),
            }),
            overrides: serde_json::json!({
                "native_provider_id": provider_id,
                "wire_api": wire_api,
                "provider_file": format!("{file_id}.config.toml"),
            }),
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        state.models.push(HarnessModel {
            native_id: model_id.to_string(),
            route,
        });
    }
    if let Some(mcps) = toml.get("mcp_servers").and_then(|m| m.as_table()) {
        for (name, spec) in mcps {
            state.mcp.push(HarnessMcp {
                native_name: name.clone(),
                server: parse_mcp(name, spec),
            });
        }
    }
}

fn push_model(
    state: &mut ParsedState,
    mid: &str,
    mv: &toml::Value,
    provider_id: &str,
    wire_api: &str,
) {
    let remote_model_id = mv
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or(mid)
        .to_string();
    let route = ModelRoute {
        id: Uuid::new_v4(),
        endpoint_id: Uuid::new_v4(),
        model_identity_id: None,
        remote_model_id,
        display_name: mid.to_string(),
        context_window: mv.get("context_window").and_then(|v| v.as_integer()),
        max_input: None,
        max_output: None,
        capabilities: serde_json::to_value(mv).unwrap_or_default(),
        overrides: serde_json::json!({
            "native_model_id": mid,
            "native_provider_id": provider_id,
            "wire_api": wire_api,
        }),
        enabled: true,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    state.models.push(HarnessModel {
        native_id: mid.to_string(),
        route,
    });
}

fn parse_mcp(name: &str, spec: &toml::Value) -> McpServer {
    let mut env: serde_json::Map<String, serde_json::Value> = Default::default();
    if let Some(env_table) = spec.get("env").and_then(|e| e.as_table()) {
        for (k, v) in env_table {
            env.insert(k.clone(), serde_json::Value::String(v.to_string()));
        }
    }
    McpServer {
        id: Uuid::new_v4(),
        name: name.to_string(),
        transport: McpTransport::Stdio,
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
        url: None,
        env,
        scope_type: ScopeType::Global,
        scope_path: None,
        provenance: serde_json::json!({"source": "codex-native"}),
        enabled: true,
    }
}
