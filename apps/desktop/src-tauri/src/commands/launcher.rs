//! Launcher: env resolution + process spawn (shared core lives here; CLI
//! reuses these functions via the same lib).

use chm_core::domain::profiles::RoleMapping;
use chm_secrets::SecretStore;
use std::collections::HashMap;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchResult {
    pub pid: Option<u32>,
    pub executable: String,
}

/// Resolves profile env entries: `$LP_<NAME>` / `${LP_<NAME>}` go through the
/// secret store; `$NAME` through the inherited environment; plain values pass
/// through untouched.
pub fn resolve_profile_env(
    env: &serde_json::Map<String, serde_json::Value>,
    secrets: &dyn SecretStore,
    inherited: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for (key, value) in env {
        let resolved = match value.as_str() {
            Some(v) if v.starts_with("$LP_") || v.starts_with("${LP_") => {
                let name = v
                    .trim_start_matches('$')
                    .trim_start_matches('{')
                    .trim_end_matches('}')
                    .trim_start_matches("LP_")
                    .to_string();
                secrets
                    .get(&name)
                    .ok()
                    .flatten()
                    .or_else(|| inherited.get(&name).cloned())
            }
            Some(v) if v.starts_with('$') => inherited.get(&v[1..]).cloned(),
            Some(v) => Some(v.to_string()),
            None => None,
        };
        if let Some(val) = resolved {
            out.insert(key.clone(), val);
        }
    }
    out
}

/// Harness-specific role→model env vars.
pub fn role_env_for(harness_type: &str, mappings: &[RoleMapping]) -> Vec<(String, String)> {
    match harness_type {
        "claude-code" => mappings
            .iter()
            .filter_map(|m| match m.role.as_str() {
                "opus" => Some(("ANTHROPIC_DEFAULT_OPUS_MODEL".to_string(), m.model.clone())),
                "sonnet" => Some((
                    "ANTHROPIC_DEFAULT_SONNET_MODEL".to_string(),
                    m.model.clone(),
                )),
                "haiku" => Some(("ANTHROPIC_DEFAULT_HAIKU_MODEL".to_string(), m.model.clone())),
                _ => None,
            })
            .collect(),
        _ => mappings
            .first()
            .map(|m| ("CHM_MODEL".to_string(), m.model.clone()))
            .into_iter()
            .collect(),
    }
}

/// Builds the final launch environment (profile wins over role mapping wins
/// over inherited).
pub fn full_launch_env(
    profile_env: HashMap<String, String>,
    role_env: Vec<(String, String)>,
    inherited: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut all = inherited.clone();
    for (k, v) in role_env {
        all.insert(k, v);
    }
    for (k, v) in profile_env {
        all.insert(k, v);
    }
    all
}
