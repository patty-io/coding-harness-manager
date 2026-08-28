//! IPC contract tests: everything that crosses the Tauri boundary must
//! serialize as plain strings/objects the TS layer expects — an externally-
//! tagged enum object (e.g. {"Custom":"gemini-cli"}) crashes React rendering.

use chm_core::domain::credentials::CredentialKind;
use chm_core::domain::harness::{HarnessInstallation, HarnessType, InstallationStatus};
use chm_core::domain::mcp::McpTransport;
use chm_core::domain::models::CatalogStatus;
use chrono::Utc;
use serde_json::Value;

fn assert_string_field(v: &Value, key: &str, expected: &str) {
    assert!(
        v.get(key) == Some(&Value::String(expected.into())),
        "{key} must serialize as string {expected:?}, got {:?}",
        v.get(key)
    );
}

#[test]
fn harness_type_wire_is_plain_string() {
    for (t, want) in [
        (HarnessType::ClaudeCode, "claude-code"),
        (HarnessType::Codex, "codex"),
        (HarnessType::OpenCode, "opencode"),
        (HarnessType::Pi, "pi"),
        (HarnessType::Reasonix, "reasonix"),
    ] {
        let v = serde_json::to_value(t).unwrap();
        assert!(v.is_string(), "enum must not be an object on the wire");
        assert_eq!(v.as_str().unwrap(), want);
    }
    let custom = serde_json::to_value(HarnessType::Custom("gemini-cli".into())).unwrap();
    assert_eq!(custom.as_str().unwrap(), "gemini-cli");
}

#[test]
fn installation_status_wire_is_kebab_case() {
    for (s, want) in [
        (InstallationStatus::Detected, "detected"),
        (InstallationStatus::Installed, "installed"),
        (InstallationStatus::ConfigMissing, "config-missing"),
        (InstallationStatus::Error, "error"),
    ] {
        let v = serde_json::to_value(s).unwrap();
        assert_eq!(v.as_str().unwrap(), want);
    }
}

#[test]
fn installation_serializes_flat_for_ts() {
    let inst = HarnessInstallation {
        id: uuid::Uuid::new_v4(),
        harness_type: HarnessType::Custom("gemini-cli".into()),
        executable_path: Some("/usr/local/bin/gemini".into()),
        version: None,
        config_path: None,
        detected_at: Utc::now(),
        last_scanned_at: None,
        status: InstallationStatus::Detected,
    };
    let v = serde_json::to_value(&inst).unwrap();
    // the exact render path that crashed: harness_type/status must be strings
    assert_string_field(&v, "harness_type", "gemini-cli");
    assert_string_field(&v, "status", "detected");
    assert!(v.get("version").is_some());
}

#[test]
fn misc_enums_wire_as_stable_strings() {
    let pairs: Vec<(serde_json::Value, &str)> = vec![
        (serde_json::to_value(McpTransport::Stdio).unwrap(), "stdio"),
        (
            serde_json::to_value(chm_core::domain::mcp::ScopeType::Global).unwrap(),
            "global",
        ),
        (
            serde_json::to_value(CatalogStatus::Available).unwrap(),
            "available",
        ),
        (
            serde_json::to_value(CredentialKind::Keychain).unwrap(),
            "keychain",
        ),
    ];
    for (v, want) in pairs {
        assert_eq!(v.as_str(), Some(want));
    }
}
