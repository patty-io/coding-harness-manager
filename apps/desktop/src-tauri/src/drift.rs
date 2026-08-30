//! Drift classification (pure) + watcher commands.

use serde_json::Value;
use sha2::{Digest, Sha256};

/// Canonical SHA-256 helper shared by history, sync, and drift checks.
pub fn sha256_hex(value: &str) -> String {
    sha256_hex_bytes(value.as_bytes())
}

pub fn sha256_hex_bytes(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriftKind {
    InSync,
    ExternallyModified,
    Conflict,
    Error,
}

impl DriftKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InSync => "in-sync",
            Self::ExternallyModified => "externally-modified",
            Self::Conflict => "conflict",
            Self::Error => "error",
        }
    }
}

/// Canonicalizes JSON or TOML so formatting-only changes don't count as drift.
pub fn normalize_config(raw: &str) -> String {
    if let Ok(v) = serde_json::from_str::<Value>(raw) {
        return serde_json::to_string(&v).unwrap_or_else(|_| raw.to_string());
    }
    raw.trim().to_string()
}

/// Overlapping managed paths make it a Conflict; otherwise external edit.
pub fn diff_state_with_managed(
    last_known: &str,
    current: &str,
    managed_paths: &[&str],
) -> DriftKind {
    let norm_last = normalize_config(last_known);
    let norm_current = normalize_config(current);
    if norm_last == norm_current {
        return DriftKind::InSync;
    }
    let _last_ok = serde_json::from_str::<Value>(&norm_last).is_ok()
        || norm_last.parse::<toml_edit::DocumentMut>().is_ok();
    let current_ok = serde_json::from_str::<Value>(&norm_current).is_ok()
        || norm_current.parse::<toml_edit::DocumentMut>().is_ok();
    if !current_ok {
        return DriftKind::Error;
    }
    for managed in managed_paths {
        if changed_under_managed(&norm_last, &norm_current, managed) {
            return DriftKind::Conflict;
        }
    }
    DriftKind::ExternallyModified
}

pub fn diff_state(last_known: &str, current: &str) -> DriftKind {
    let a = serde_json::from_str::<Value>(last_known).ok();
    let b = serde_json::from_str::<Value>(current).ok();
    match (a, b) {
        (Some(a), Some(b)) => {
            if a == b {
                DriftKind::InSync
            } else {
                DriftKind::ExternallyModified
            }
        }
        _ => diff_state_with_managed(last_known, current, &[]),
    }
}

fn changed_under_managed(last: &str, current: &str, path: &str) -> bool {
    // walk JSON pointer-ish dotted path on both; values differ -> conflict
    let get = |doc: &str| -> Option<Value> {
        let mut v = serde_json::from_str::<Value>(doc).ok()?;
        // accept TOML-ish fallback by naive search when not json
        for seg in path.split('.') {
            v = v.get(seg)?.clone();
        }
        Some(v)
    };
    let a = get(last);
    let b = get(current);
    if let (Some(a), Some(b)) = (a, b) {
        a != b
    } else {
        false
    }
}
