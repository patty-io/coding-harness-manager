use chm_core::domain::harness::{HarnessInstallation, HarnessType, InstallationStatus};
use chm_harness_sdk::adapter::types::HarnessAdapter;
use pi_adapter::PiAdapter;
use pi_adapter::writer;
use serde_json::Value;
use std::path::PathBuf;

fn fixture_dir() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("fixtures/pi");
    p
}

fn normalize_fixture_paths(value: &mut Value) {
    match value {
        Value::String(path) => {
            if let Some(index) = path.find("/fixtures/pi/") {
                *path = format!("<repo>{}", &path[index..]);
            }
        }
        Value::Array(values) => {
            for value in values {
                normalize_fixture_paths(value);
            }
        }
        Value::Object(values) => {
            for value in values.values_mut() {
                normalize_fixture_paths(value);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn install(config_file: PathBuf, _home: PathBuf) -> HarnessInstallation {
    HarnessInstallation {
        id: uuid::Uuid::new_v4(),
        harness_type: HarnessType::Pi,
        executable_path: Some("/fake/pi".into()),
        version: Some("0.84.3".into()),
        config_path: Some(config_file.display().to_string()),
        detected_at: chrono::Utc::now(),
        last_scanned_at: None,
        status: InstallationStatus::Installed,
    }
}

#[test]
fn pi_full_config_parses_without_warnings() {
    let dir = fixture_dir();
    let versions: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    assert!(
        !versions.is_empty(),
        "fixtures/pi is empty — run phase 0 first"
    );

    for version_dir in versions {
        let golden_path = format!(
            "tests/golden/{}.json",
            version_dir.file_name().unwrap().to_string_lossy()
        );
        let home = version_dir.join("home");
        let config_file = home.join(".pi/agent/models.json");
        let adapter = PiAdapter;
        let inst = install(config_file, home.clone());
        let state = adapter.read_state(&inst).expect("read_state ok");
        assert!(state.warnings.is_empty(), "warnings: {:?}", state.warnings);

        let mut expected: Value = serde_json::from_str(
            &std::fs::read_to_string(&golden_path)
                .unwrap_or_else(|_| panic!("golden missing for {golden_path}")),
        )
        .unwrap();
        let mut actual = serde_json::json!({
            "models": state.models.iter().map(|m| serde_json::json!({
                "native_id": m.native_id,
                "display_name": m.route.display_name,
                "remote_model_id": m.route.remote_model_id,
                "context_window": m.route.context_window,
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
            "profiles": state.profiles,
            "skills": state.skills.iter().map(|s| serde_json::json!({
                "name": s.name,
                "path": s.path,
                "symlinked": s.symlinked,
            })).collect::<Vec<_>>(),
        });
        normalize_fixture_paths(&mut expected);
        normalize_fixture_paths(&mut actual);
        assert_eq!(
            actual,
            expected,
            "golden mismatch for {}",
            version_dir.display()
        );
    }
}

#[test]
fn writer_updates_and_removes_models() {
    let raw = r#"{
        "providers": {
            "pattycode": {
                "baseUrl": "https://omni.agents.patty.io/v1",
                "models": [
                    {"id": "gl/glm-5.2", "name": "GLM 5.2"},
                    {"id": "mm/minimax-m3", "name": "MiniMax M3"}
                ]
            },
            "yolo": {"baseUrl": "https://yolo.example/v1", "models": [{"id": "qwen", "name": "Qwen"}]}
        }
    }"#;
    let mut doc = writer::parse_document(raw).unwrap();

    // update: matched by id under any provider
    assert!(writer::update_model(
        &mut doc,
        "gl/glm-5.2",
        "GLM 5.2 (edited)",
        Some(200000)
    ));
    let models = &doc["providers"]["pattycode"]["models"];
    assert_eq!(models[0]["name"], "GLM 5.2 (edited)");
    assert_eq!(models[0]["contextWindow"], 200_000);

    // update with None context removes the field
    assert!(writer::update_model(
        &mut doc,
        "gl/glm-5.2",
        "GLM 5.2",
        None
    ));
    assert!(
        doc["providers"]["pattycode"]["models"][0]
            .get("contextWindow")
            .is_none()
    );

    // unknown id is reported, not invented
    assert!(!writer::update_model(&mut doc, "nope", "x", None));

    // remove drops the entry, leaves siblings and the provider
    assert_eq!(writer::remove_model(&mut doc, "mm/minimax-m3"), 1);
    let ids: Vec<&str> = doc["providers"]["pattycode"]["models"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["gl/glm-5.2"]);
    assert!(doc["providers"]["yolo"]["models"].as_array().unwrap().len() == 1);

    // removing a nonexistent id reports zero
    assert_eq!(writer::remove_model(&mut doc, "mm/minimax-m3"), 0);
}

#[test]
fn writer_emits_explicit_environment_interpolation() {
    let mut doc = writer::parse_document(r#"{"providers":{"proxy":{"models":[]}}}"#).unwrap();
    writer::configure_provider_auth(&mut doc, "proxy", Some("env"), Some("PROXY_API_KEY"));
    assert_eq!(doc["providers"]["proxy"]["apiKey"], "$PROXY_API_KEY");
}

#[test]
fn malformed_provider_shapes_return_errors_instead_of_panicking() {
    assert!(
        writer::parse_document(r#"{"providers":null}"#)
            .unwrap_err()
            .contains("providers must be an object")
    );
    assert!(
        writer::parse_document(r#"{"providers":{"broken":null}}"#)
            .unwrap_err()
            .contains("provider broken must be an object")
    );
    assert!(
        writer::parse_document(r#"{"providers":{"broken":{"models":{}}}}"#)
            .unwrap_err()
            .contains("provider broken models must be an array")
    );
}

#[test]
fn removing_a_providers_last_model_prunes_only_an_empty_stub() {
    let raw = r#"{
        "providers": {
            "custom": {
                "models": [{"id": "copied-model", "name": "Copied model"}]
            },
            "configured": {
                "baseUrl": "https://configured.example/v1",
                "models": [{"id": "only-model", "name": "Only model"}]
            }
        }
    }"#;
    let mut doc = writer::parse_document(raw).unwrap();

    assert_eq!(writer::remove_model(&mut doc, "copied-model"), 1);
    assert!(doc["providers"].get("custom").is_none());

    assert_eq!(writer::remove_model(&mut doc, "only-model"), 1);
    assert_eq!(
        doc["providers"]["configured"]["baseUrl"],
        "https://configured.example/v1"
    );
    assert_eq!(
        doc["providers"]["configured"]["models"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
}

#[test]
fn validation_rejects_provider_without_required_configuration() {
    let path = std::env::temp_dir().join(format!(
        "pi-invalid-empty-provider-{}.json",
        uuid::Uuid::new_v4()
    ));
    std::fs::write(&path, r#"{"providers":{"custom":{"models":[]}}}"#).unwrap();

    let report = writer::validate_config(path.to_str().unwrap());
    let _ = std::fs::remove_file(path);

    assert!(!report.ok);
    assert!(
        report.errors.iter().any(|error| error.contains("custom")),
        "errors: {:?}",
        report.errors
    );
}

#[test]
fn provider_scoped_writer_does_not_touch_sibling_provider() {
    let raw = r#"{
        "providers": {
            "alpha": {"models": [{"id": "same", "name": "Alpha"}]},
            "beta": {"models": [{"id": "same", "name": "Beta"}]}
        }
    }"#;
    let mut doc = writer::parse_document(raw).unwrap();
    assert!(writer::update_model_in_provider(
        &mut doc,
        Some("beta"),
        "same",
        "Beta edited",
        None,
    ));
    assert_eq!(doc["providers"]["alpha"]["models"][0]["name"], "Alpha");
    assert_eq!(doc["providers"]["beta"]["models"][0]["name"], "Beta edited");
    assert_eq!(
        writer::remove_model_in_provider(&mut doc, Some("beta"), "same"),
        1
    );
    assert_eq!(
        doc["providers"]["alpha"]["models"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
