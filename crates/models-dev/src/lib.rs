//! models.dev client + confidence-scored matching.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MdError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("parse error: {0}")]
    Parse(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelsDevModel {
    pub id: String,
    pub name: String,
    pub provider: Option<String>,
    pub context_window: Option<i64>,
    pub max_output: Option<i64>,
    pub modalities: serde_json::Value,
}

#[derive(Debug, Clone, Default)]
pub struct ModelsDevCatalog {
    pub models: Vec<ModelsDevModel>,
}

pub async fn fetch_catalog(http: &reqwest::Client) -> Result<ModelsDevCatalog, MdError> {
    let resp = http.get("https://models.dev/api.json").send().await?;
    let raw: serde_json::Value = resp.json().await?;
    Ok(parse_catalog(&raw))
}

fn parse_catalog(raw: &serde_json::Value) -> ModelsDevCatalog {
    let mut models = Vec::new();
    if let Some(providers) = raw.as_object() {
        for (pid, pv) in providers {
            let provider_name = pv
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or(pid)
                .to_string();
            if let Some(map) = pv.get("models").and_then(|m| m.as_object()) {
                for (id, meta) in map {
                    models.push(ModelsDevModel {
                        id: id.clone(),
                        name: meta
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or(id)
                            .to_string(),
                        provider: Some(provider_name.clone()),
                        context_window: meta.get("context").and_then(|v| v.as_i64()),
                        max_output: meta.get("max_output").and_then(|v| v.as_i64()),
                        modalities: meta.clone(),
                    });
                }
            }
        }
    }
    ModelsDevCatalog { models }
}

/// Parses the bundled fixture catalog (committed with the crate), cached so
/// the 4.3MB JSON is parsed exactly once per process.
pub fn bundled_catalog() -> ModelsDevCatalog {
    static CACHE: std::sync::OnceLock<ModelsDevCatalog> = std::sync::OnceLock::new();
    CACHE
        .get_or_init(|| {
            let raw = include_str!("../fixtures/catalog.json");
            let parsed: serde_json::Value = serde_json::from_str(raw).unwrap_or_default();
            parse_catalog(&parsed)
        })
        .clone()
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchResult {
    pub confidence: u8,
    pub model: Option<ModelsDevModel>,
}

/// 100 exact | 95 alias | 85 normalized | 60 candidate | 0 unknown.
pub fn match_model(remote_id: &str, catalog: &ModelsDevCatalog) -> MatchResult {
    for m in &catalog.models {
        if m.id == remote_id {
            return MatchResult {
                confidence: 100,
                model: Some(m.clone()),
            };
        }
    }
    // known alias: openrouter-style "provider/model"
    let stripped = remote_id.split('/').next_back().unwrap_or(remote_id);
    for m in &catalog.models {
        if m.id == stripped {
            return MatchResult {
                confidence: 95,
                model: Some(m.clone()),
            };
        }
    }
    // normalized: lowercase, keep alnum only
    let norm = |s: &str| {
        s.to_lowercase()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
    };
    let target = norm(stripped);
    for m in &catalog.models {
        if norm(&m.id) == target {
            return MatchResult {
                confidence: 85,
                model: Some(m.clone()),
            };
        }
    }
    // candidate: exact family name appears in a known model id
    if let Some(family) = target.rsplit(|c: char| c.is_ascii_digit()).next()
        && !family.is_empty()
        && family.len() >= 4
    {
        for m in &catalog.models {
            if norm(&m.id).contains(family) {
                return MatchResult {
                    confidence: 60,
                    model: Some(m.clone()),
                };
            }
        }
    }
    MatchResult {
        confidence: 0,
        model: None,
    }
}
