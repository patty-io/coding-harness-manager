//! Cross-harness detection for MCP servers and skills.
//!
//! Reads every harness installation via its adapter, collects what is
//! configured on disk, dedupes by identity (name + transport + target for
//! MCP; name for skills), and reports where each item was found plus whether
//! it is already in the library.

use chm_database::repos::harness::list_installations;
use serde::Serialize;
use std::collections::HashMap;
use tauri::State;

use crate::AppState;
use adapters::all_adapters;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedMcp {
    pub name: String,
    pub transport: String,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
    pub env: serde_json::Map<String, serde_json::Value>,
    pub found_in: Vec<String>,
    pub in_library: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedSkill {
    pub name: String,
    pub found_in: Vec<String>,
    pub paths: Vec<String>,
    pub in_library: bool,
}

fn adapter_for(
    harness_type: &str,
) -> Option<Box<dyn chm_harness_sdk::adapter::types::HarnessAdapter>> {
    all_adapters().into_iter().find(|a| a.id() == harness_type)
}

async fn collect_states(
    state: &State<'_, AppState>,
) -> Result<Vec<(String, chm_harness_sdk::adapter::types::ParsedState)>, String> {
    let mut out = Vec::new();
    for inst in list_installations(&state.pool)
        .await
        .map_err(|e| e.to_string())?
    {
        let Some(adapter) = adapter_for(inst.harness_type.as_str()) else {
            continue;
        };
        match adapter.read_state(&inst) {
            Ok(parsed) => out.push((inst.harness_type.as_str().to_string(), parsed)),
            Err(_) => continue, // unreadable harness — skip, don't fail detection
        }
    }
    Ok(out)
}

#[tauri::command]
pub async fn detect_mcp_cmd(state: State<'_, AppState>) -> Result<Vec<DetectedMcp>, String> {
    let library = chm_database::repos::mcp::list_mcp_servers(&state.pool)
        .await
        .map_err(|e| e.to_string())?;
    let library_names: std::collections::HashSet<String> =
        library.iter().map(|s| s.name.to_lowercase()).collect();

    // identity (lowercase name + transport + command/url) -> entry
    let mut found: HashMap<String, DetectedMcp> = HashMap::new();
    for (htype, parsed) in collect_states(&state).await? {
        for m in parsed.mcp {
            let key = format!(
                "{}|{}|{}",
                m.native_name.to_lowercase(),
                m.server.transport.as_str(),
                m.server
                    .command
                    .clone()
                    .or(m.server.url.clone())
                    .unwrap_or_default()
            );
            let entry = found.entry(key).or_insert_with(|| DetectedMcp {
                name: m.native_name.clone(),
                transport: m.server.transport.as_str().to_string(),
                command: m.server.command.clone(),
                args: m.server.args.clone(),
                url: m.server.url.clone(),
                env: m.server.env.clone(),
                found_in: Vec::new(),
                in_library: library_names.contains(&m.native_name.to_lowercase()),
            });
            if !entry.found_in.contains(&htype) {
                entry.found_in.push(htype.clone());
            }
        }
    }
    let mut out: Vec<DetectedMcp> = found.into_values().collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

#[tauri::command]
pub async fn detect_skills_cmd(state: State<'_, AppState>) -> Result<Vec<DetectedSkill>, String> {
    let library = chm_database::repos::skills::list_skills(&state.pool)
        .await
        .map_err(|e| e.to_string())?;
    let library_names: std::collections::HashSet<String> =
        library.iter().map(|s| s.name.to_lowercase()).collect();

    let mut found: HashMap<String, DetectedSkill> = HashMap::new();
    for (htype, parsed) in collect_states(&state).await? {
        for s in parsed.skills {
            let key = s.name.to_lowercase();
            let entry = found.entry(key).or_insert_with(|| DetectedSkill {
                name: s.name.clone(),
                found_in: Vec::new(),
                paths: Vec::new(),
                in_library: library_names.contains(&s.name.to_lowercase()),
            });
            if !entry.found_in.contains(&htype) {
                entry.found_in.push(htype.clone());
            }
            if !entry.paths.contains(&s.path) {
                entry.paths.push(s.path.clone());
            }
        }
    }
    let mut out: Vec<DetectedSkill> = found.into_values().collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}
