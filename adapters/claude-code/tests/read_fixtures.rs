use chm_core::domain::harness::{HarnessInstallation, HarnessType, InstallationStatus};
use chm_harness_sdk::adapter::types::HarnessAdapter;
use claude_code_adapter::ClaudeCodeAdapter;
use serde_json::Value;
use std::path::PathBuf;

fn fixture_home() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.join("fixtures/claude-code/2.1.246/home")
}

#[test]
fn claude_full_config_parses_without_warnings() {
    let home = fixture_home();
    let settings = home.join(".claude/settings.json");
    assert!(settings.exists(), "fixture missing — run phase 0 first");
    let inst = HarnessInstallation {
        id: uuid::Uuid::new_v4(),
        harness_type: HarnessType::ClaudeCode,
        executable_path: Some("/fake/claude".into()),
        version: Some("2.1.246".into()),
        config_path: Some(settings.display().to_string()),
        detected_at: chrono::Utc::now(),
        last_scanned_at: None,
        status: InstallationStatus::Installed,
    };
    let state = ClaudeCodeAdapter.read_state(&inst).expect("read_state ok");
    assert!(state.warnings.is_empty(), "warnings: {:?}", state.warnings);

    let expected: Value = serde_json::from_str(
        &std::fs::read_to_string("tests/golden/2.1.246.json").expect("golden missing"),
    )
    .unwrap();
    let actual = serde_json::json!({
        "models": state.models.iter().map(|m| serde_json::json!({
            "native_id": m.native_id,
            "remote_model_id": m.route.remote_model_id,
            "display_name": m.route.display_name,
            "capabilities": m.route.capabilities,
        })).collect::<Vec<_>>(),
        "providers": state.providers,
        "mcp": state.mcp.iter().map(|m| serde_json::json!({
            "native_name": m.native_name,
            "transport": m.server.transport.as_str(),
            "command": m.server.command,
            "args": m.server.args,
            "url": m.server.url,
            "env": m.server.env,
        })).collect::<Vec<_>>(),
        "skills": state.skills.iter().map(|s| serde_json::json!({
            "name": s.name,
            "symlinked": s.symlinked,
        })).collect::<Vec<_>>(),
    });
    assert_eq!(actual, expected);
}

#[test]
fn claude_gateway_model_and_base_url_are_read_back() {
    let state = claude_code_adapter::parser::parse_config(
        Some(
            r#"{
              "env": {
                "ANTHROPIC_MODEL": "custom/model-v2",
                "ANTHROPIC_BASE_URL": "https://gateway.example/v1",
                "DISABLE_AUTOUPDATER": "1"
              }
            }"#,
        ),
        None,
        std::path::Path::new("/tmp"),
    )
    .expect("parse gateway settings");
    assert_eq!(state.models.len(), 1);
    assert_eq!(state.models[0].native_id, "custom/model-v2");
    assert_eq!(state.models[0].route.remote_model_id, "custom/model-v2");
    assert_eq!(
        state.models[0].route.overrides["base_url"],
        "https://gateway.example/v1"
    );
    assert_eq!(
        state.models[0].route.overrides["protocol"],
        "anthropic-messages"
    );
    assert_eq!(state.providers.len(), 2);
}
