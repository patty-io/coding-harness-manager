use chm_core::domain::harness::{HarnessInstallation, HarnessType, InstallationStatus};
use chm_harness_sdk::adapter::types::HarnessAdapter;
use opencode_adapter::OpenCodeAdapter;
use opencode_adapter::writer;
use serde_json::Value;
use std::path::PathBuf;

fn fixture_dir() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop(); // adapters/opencode -> repo root
    p.push("fixtures/opencode");
    p
}

fn install(config_file: PathBuf) -> HarnessInstallation {
    HarnessInstallation {
        id: uuid::Uuid::new_v4(),
        harness_type: HarnessType::OpenCode,
        executable_path: Some("/fake/opencode".into()),
        version: Some("1.18.23".into()),
        config_path: Some(config_file.display().to_string()),
        detected_at: chrono::Utc::now(),
        last_scanned_at: None,
        status: InstallationStatus::Installed,
    }
}

#[test]
fn opencode_full_config_parses_without_warnings() {
    let dir = fixture_dir();
    let versions: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    assert!(
        !versions.is_empty(),
        "fixtures/opencode is empty — run phase 0 first"
    );

    for version_dir in versions {
        let golden_path = format!(
            "tests/golden/{}.json",
            version_dir.file_name().unwrap().to_string_lossy()
        );
        let config_file = version_dir.join("opencode-full.jsonc");
        let adapter = OpenCodeAdapter;
        let inst = install(config_file);
        let state = adapter.read_state(&inst).expect("read_state ok");
        assert!(state.warnings.is_empty(), "warnings: {:?}", state.warnings);

        let expected: Value = serde_json::from_str(
            &std::fs::read_to_string(&golden_path)
                .unwrap_or_else(|_| panic!("golden missing for {golden_path}")),
        )
        .unwrap();
        let actual = serde_json::json!({
            "models": state.models.iter().map(|m| serde_json::json!({
                "native_id": m.native_id,
                "display_name": m.route.display_name,
                "remote_model_id": m.route.remote_model_id,
                "context_window": m.route.context_window,
                "max_output": m.route.max_output,
                "capabilities": m.route.capabilities,
            })).collect::<Vec<_>>(),
            "mcp": state.mcp.iter().map(|m| serde_json::json!({
                "native_name": m.native_name,
                "transport": m.server.transport.as_str(),
                "command": m.server.command,
                "args": m.server.args,
                "url": m.server.url,
                "env": m.server.env,
                "enabled": m.server.enabled,
            })).collect::<Vec<_>>(),
            "skills": state.skills.iter().map(|s| serde_json::json!({
                "name": s.name,
                "symlinked": s.symlinked,
            })).collect::<Vec<_>>(),
        });
        assert_eq!(
            actual,
            expected,
            "golden mismatch for {}",
            version_dir.display()
        );
    }
}

#[test]
fn writer_migrates_legacy_custom_model_to_its_declared_provider() {
    let raw = r#"{
        "provider": {
            "custom": {
                "models": {"qwen3.8-27b": {"name": "qwen3.8-27b"}}
            }
        }
    }"#;
    let mut doc = writer::parse_document(raw).unwrap();

    writer::fold_model(
        &mut doc,
        "yolo-auto",
        "qwen3.8-27b",
        "Qwen 3.8 27B",
        None,
        &serde_json::json!({}),
    );

    assert!(
        doc["provider"]["custom"]["models"]
            .get("qwen3.8-27b")
            .is_none()
    );
    assert_eq!(
        doc["provider"]["yolo-auto"]["models"]["qwen3.8-27b"]["name"],
        serde_json::json!("Qwen 3.8 27B")
    );
}

#[test]
fn writer_configures_a_discovered_openai_provider_without_copying_secrets() {
    let mut doc = writer::parse_document("{}").unwrap();

    writer::fold_model_with_provider(
        &mut doc,
        "yolo-auto",
        "qwen3.8-27b",
        "qwen3.8-27b",
        None,
        &serde_json::json!({}),
        Some(&serde_json::json!({
            "display_name": "Yolo-Auto",
            "base_url": "https://yolo-auto.example/v1",
            "protocol": "openai-chat",
            "api_key_env": "YOLO_AUTO_API_KEY"
        })),
    );

    let provider = &doc["provider"]["yolo-auto"];
    assert_eq!(provider["name"], serde_json::json!("Yolo-Auto"));
    assert_eq!(
        provider["npm"],
        serde_json::json!("@ai-sdk/openai-compatible")
    );
    assert_eq!(
        provider["options"]["baseURL"],
        serde_json::json!("https://yolo-auto.example/v1")
    );
    assert_eq!(
        provider["options"]["apiKey"],
        serde_json::json!("{env:YOLO_AUTO_API_KEY}")
    );
    assert!(provider.get("secret").is_none());
}
