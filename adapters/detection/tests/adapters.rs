use chm_core::domain::harness::{HarnessInstallation, HarnessType, InstallationStatus};
use chm_harness_sdk::adapter::plan::{
    AddAction, PlanAction, ReconciliationPlan, RemoveAction, UpdateAction,
};
use chm_harness_sdk::adapter::types::HarnessAdapter;
use detection_adapter::{
    AiderAdapter, AmpAdapter, ClineAdapter, ContinueAdapter, CursorAdapter, GeminiAdapter,
    GooseAdapter, KimiAdapter, QwenAdapter, RooAdapter,
};
use std::path::PathBuf;

fn install(id: &str, config_path: &str) -> HarnessInstallation {
    HarnessInstallation {
        id: uuid::Uuid::new_v4(),
        harness_type: HarnessType::parse_str(id),
        executable_path: Some(format!("/fake/{id}")),
        version: Some("1.0.0".into()),
        config_path: Some(config_path.into()),
        detected_at: chrono::Utc::now(),
        last_scanned_at: None,
        status: InstallationStatus::Installed,
    }
}

#[test]
fn kimi_reads_provider_scoped_models_without_secrets() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    std::fs::write(
        &config,
        r#"
default_model = "managed:kimi-code/k3"
[providers."managed:kimi-code"]
type = "kimi"
base_url = "https://api.kimi.com/coding/v1"
api_key = "sk-live-secret"
[models."managed:kimi-code/k3"]
provider = "managed:kimi-code"
model = "k3"
max_context_size = 1048576
display_name = "K3"
"#,
    )
    .unwrap();
    let state = KimiAdapter
        .read_state(&install("kimi-cli", &config.display().to_string()))
        .unwrap();
    assert_eq!(state.models.len(), 1);
    assert_eq!(state.models[0].native_id, "managed:kimi-code/k3");
    assert_eq!(state.models[0].route.context_window, Some(1_048_576));
    assert_eq!(
        state.providers[0]["base_url"],
        "https://api.kimi.com/coding/v1"
    );
    assert!(state.providers[0].get("api_key").is_none());
}

#[test]
fn kimi_reads_and_writes_explicit_json_config_files() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.json");
    std::fs::write(
        &config,
        r#"{"providers":{"openai":{"base_url":"https://api.example/v1","api_key":"secret"}},"models":{"old":{"provider":"openai","model":"old","max_context_size":4096}}}"#,
    )
    .unwrap();
    let install = install("kimi-cli", &config.display().to_string());
    let state = KimiAdapter.read_state(&install).unwrap();
    assert_eq!(state.models.len(), 1);
    assert_eq!(state.models[0].native_id, "old");
    assert!(state.providers[0].get("api_key").is_none());
    let native = KimiAdapter
        .plan(
            &ReconciliationPlan {
                actions: vec![PlanAction::Add(AddAction {
                    kind: "model".into(),
                    identity: "new".into(),
                    payload: serde_json::json!({
                        "native_provider_id":"openai",
                        "remote_model_id":"new",
                        "display_name":"New",
                        "context_window":8192
                    }),
                    native_provider_id: Some("openai".into()),
                })],
            },
            &install,
        )
        .unwrap();
    let after = native.changes[0].after.as_deref().unwrap();
    assert!(after.contains("\"new\""));
    assert!(after.contains("\"max_context_size\": 8192"));
    assert!(after.contains("\"api_key\": \"secret\""));
}

#[test]
fn continue_reads_models_and_mcp_from_yaml() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.yaml");
    std::fs::write(
        &config,
        r#"
name: Demo
version: 1.0.0
schema: v1
models:
  - name: local
    provider: openai
    model: qwen3
    apiBase: https://llm.example/v1
    defaultCompletionOptions:
      contextLength: 32768
mcpServers:
  - name: tools
    command: uvx
    args: [mcp-server]
    env:
      API_KEY: $TOOLS_KEY
"#,
    )
    .unwrap();
    let state = ContinueAdapter
        .read_state(&install("continue", &config.display().to_string()))
        .unwrap();
    assert_eq!(state.models.len(), 1);
    assert_eq!(state.models[0].route.remote_model_id, "qwen3");
    assert_eq!(state.models[0].route.context_window, Some(32768));
    assert_eq!(state.mcp.len(), 1);
    assert_eq!(state.mcp[0].server.command.as_deref(), Some("uvx"));
}

#[test]
fn every_detection_adapter_has_a_real_capability_surface() {
    let adapters: Vec<Box<dyn HarnessAdapter>> = vec![
        Box::new(GeminiAdapter),
        Box::new(QwenAdapter),
        Box::new(KimiAdapter),
        Box::new(CursorAdapter),
        Box::new(ClineAdapter),
        Box::new(RooAdapter),
        Box::new(AiderAdapter),
        Box::new(AmpAdapter),
        Box::new(GooseAdapter),
        Box::new(ContinueAdapter),
    ];
    let ids: Vec<_> = adapters.iter().map(|a| a.id()).collect();
    assert_eq!(ids.len(), 10);
    for adapter in adapters {
        let caps = adapter.capabilities();
        assert!(
            caps.supports_mcp_global
                || caps.supports_custom_models
                || caps.supports_profiles
                || caps.supports_runtime_env,
            "{} has no declared supported surface",
            adapter.id()
        );
    }
}

#[test]
fn json_settings_adapter_reads_mcp_without_inventing_models() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("settings.json");
    std::fs::write(
        &config,
        r#"{
          "mcpServers": {
            "remote": {"httpUrl": "https://mcp.example/mcp", "headers": {"Authorization": "Bearer secret"}}
          }
        }"#,
    )
    .unwrap();
    let state = GeminiAdapter
        .read_state(&install("gemini-cli", &config.display().to_string()))
        .unwrap();
    assert!(state.models.is_empty());
    assert_eq!(state.mcp.len(), 1);
    assert_eq!(
        state.mcp[0].server.url.as_deref(),
        Some("https://mcp.example/mcp")
    );
    assert_eq!(
        state.mcp[0].server.env["headers"]["Authorization"],
        "<redacted>"
    );
}

#[test]
fn adapters_are_send_sync_and_paths_are_not_required_to_be_home() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<KimiAdapter>();
    assert!(PathBuf::from("/tmp/fixture").is_absolute());
}

#[test]
fn kimi_plan_round_trips_model_add_update_remove() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    std::fs::write(
        &config,
        r#"[providers.openai]
type = "openai"
base_url = "https://api.example/v1"
api_key = "keep-me-on-disk"

[models.old]
provider = "openai"
model = "old"
max_context_size = 4096
"#,
    )
    .unwrap();
    let plan = ReconciliationPlan {
        actions: vec![
            PlanAction::Add(AddAction {
                kind: "model".into(),
                identity: "new".into(),
                payload: serde_json::json!({
                    "native_provider_id": "openai",
                    "remote_model_id": "new",
                    "display_name": "New",
                    "context_window": 8192,
                }),
                native_provider_id: Some("openai".into()),
            }),
            PlanAction::Update(UpdateAction {
                kind: "model".into(),
                identity: "old".into(),
                changed_fields: vec!["display_name".into()],
                desired: serde_json::json!({"display_name": "Old renamed", "context_window": 1234}),
                current: serde_json::json!({}),
                native_provider_id: Some("openai".into()),
            }),
            PlanAction::Remove(RemoveAction {
                kind: "model".into(),
                identity: "new".into(),
                native_provider_id: Some("openai".into()),
            }),
        ],
    };
    let native = KimiAdapter
        .plan(&plan, &install("kimi-cli", &config.display().to_string()))
        .unwrap();
    assert_eq!(native.changes.len(), 1);
    let after = native.changes[0].after.as_deref().unwrap();
    assert!(after.contains("api_key = \"keep-me-on-disk\""));
    assert!(!after.contains("[models.new]"));
    assert!(after.contains("display_name = \"Old renamed\""));
    assert!(after.contains("max_context_size = 1234"));
}

#[test]
fn kimi_update_clears_nullable_limits() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    std::fs::write(
        &config,
        "[providers.openai]\nbase_url = \"https://api.example/v1\"\n[models.old]\nprovider = \"openai\"\nmodel = \"old\"\nmax_context_size = 4096\nmax_output_size = 2048\n",
    )
    .unwrap();
    let native = KimiAdapter
        .plan(
            &ReconciliationPlan {
                actions: vec![PlanAction::Update(UpdateAction {
                    kind: "model".into(),
                    identity: "old".into(),
                    changed_fields: vec!["context_window".into(), "max_output".into()],
                    desired: serde_json::json!({
                        "context_window": null,
                        "max_output": null
                    }),
                    current: serde_json::json!({}),
                    native_provider_id: Some("openai".into()),
                })],
            },
            &install("kimi-cli", &config.display().to_string()),
        )
        .unwrap();
    let after = native.changes[0].after.as_deref().unwrap();
    assert!(!after.contains("max_context_size"));
    assert!(!after.contains("max_output_size"));
}

#[test]
fn continue_plan_writes_mcp_as_native_yaml() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.yaml");
    std::fs::write(
        &config,
        "name: Demo\nversion: 1.0.0\nschema: v1\nmodels: []\nmcpServers: []\n",
    )
    .unwrap();
    let server = chm_core::domain::mcp::McpServer {
        id: uuid::Uuid::new_v4(),
        name: "tools".into(),
        transport: chm_core::domain::mcp::McpTransport::Stdio,
        command: Some("uvx".into()),
        args: vec!["mcp-server".into()],
        url: None,
        env: serde_json::Map::new(),
        scope_type: chm_core::domain::mcp::ScopeType::Global,
        scope_path: None,
        provenance: serde_json::json!({"source": "test"}),
        enabled: true,
    };
    let plan = ReconciliationPlan {
        actions: vec![PlanAction::Add(AddAction {
            kind: "mcp".into(),
            identity: "tools".into(),
            payload: serde_json::to_value(server).unwrap(),
            native_provider_id: None,
        })],
    };
    let native = ContinueAdapter
        .plan(&plan, &install("continue", &config.display().to_string()))
        .unwrap();
    let after = native.changes[0].after.as_deref().unwrap();
    assert!(after.contains("name: tools"));
    assert!(after.contains("command: uvx"));
}

#[test]
fn selection_only_harnesses_do_not_fabricate_model_rows() {
    let dir = tempfile::tempdir().unwrap();
    let qwen = dir.path().join("qwen-settings.json");
    std::fs::write(&qwen, r#"{"model":"qwen3-coder","mcpServers":{}}"#).unwrap();
    let state = QwenAdapter
        .read_state(&install("qwen-code", &qwen.display().to_string()))
        .unwrap();
    assert!(state.models.is_empty());
    assert_eq!(state.profiles[0]["model"], "qwen3-coder");

    let aider = dir.path().join("aider.conf.yml");
    std::fs::write(&aider, "model: openai/gpt-4.1\nweak-model: gpt-4o-mini\n").unwrap();
    let state = AiderAdapter
        .read_state(&install("aider", &aider.display().to_string()))
        .unwrap();
    assert_eq!(state.models.len(), 1);
    assert_eq!(state.models[0].native_id, "gpt-4.1");
    assert_eq!(state.profiles.len(), 2);
}

#[test]
fn goose_custom_provider_models_are_read_and_written_without_credentials() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.yaml");
    std::fs::write(&config, "provider: custom_corp\nmodel: gpt-4o\n").unwrap();
    let provider_dir = dir.path().join(".config/goose/custom_providers");
    std::fs::create_dir_all(&provider_dir).unwrap();
    let provider = provider_dir.join("custom_corp.json");
    std::fs::write(
        &provider,
        r#"{
          "name":"custom_corp",
          "engine":"openai",
          "display_name":"Corporate",
          "api_key_env":"CORP_KEY",
          "base_url":"https://llm.example/v1",
          "models":[{"name":"gpt-4o","alias":"GPT-4o","context_limit":128000}]
        }"#,
    )
    .unwrap();
    let state = GooseAdapter
        .read_state(&install("goose", &config.display().to_string()))
        .unwrap();
    assert_eq!(state.models.len(), 1);
    assert_eq!(state.models[0].route.display_name, "GPT-4o");
    assert!(state.providers[0].get("api_key").is_none());

    let action = PlanAction::Update(UpdateAction {
        kind: "model".into(),
        identity: "gpt-4o".into(),
        changed_fields: vec!["display_name".into(), "context_window".into()],
        desired: serde_json::json!({
            "display_name":"GPT-4o tuned",
            "context_window": 256000,
            "overrides": {"native_provider_id":"custom_corp", "config_file":provider.display().to_string()}
        }),
        current: serde_json::json!({}),
        native_provider_id: Some("custom_corp".into()),
    });
    let native = GooseAdapter
        .plan(
            &ReconciliationPlan {
                actions: vec![action],
            },
            &install("goose", &config.display().to_string()),
        )
        .unwrap();
    assert_eq!(native.changes.len(), 1);
    let after = native.changes[0].after.as_deref().unwrap();
    assert!(after.contains("GPT-4o tuned"));
    assert!(after.contains("256000"));
    assert!(
        after.contains("CORP_KEY"),
        "credential references must be preserved"
    );
}

#[test]
fn goose_new_custom_provider_writes_native_config_and_protected_secret_plan() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.yaml");
    std::fs::write(&config, "provider: yolo-auto\n").unwrap();
    let credential_ref_id = uuid::Uuid::new_v4();
    let native = GooseAdapter
        .plan(
            &ReconciliationPlan {
                actions: vec![PlanAction::Add(AddAction {
                    kind: "model".into(),
                    identity: "qwen3.8-27b".into(),
                    payload: serde_json::json!({
                        "native_provider_id": "yolo-auto",
                        "remote_model_id": "qwen3.8-27b",
                        "display_name": "Qwen 3.8 27B",
                        "context_window": 131072,
                        "base_url": "https://yolo-auto.example/v1",
                        "protocol": "openai-chat",
                        "api_key_env": "YOLO_AUTO_API_KEY",
                        "credential_ref_id": credential_ref_id,
                        "overrides": {
                            "native_provider_config": {
                                "base_url": "https://yolo-auto.example/v1",
                                "protocol": "openai-chat"
                            }
                        }
                    }),
                    native_provider_id: Some("yolo-auto".into()),
                })],
            },
            &install("goose", &config.display().to_string()),
        )
        .unwrap();
    assert_eq!(
        native.changes.len(),
        2,
        "provider JSON + keyring mode config"
    );
    let provider_change = native
        .changes
        .iter()
        .find(|change| {
            change
                .file_path
                .ends_with("custom_providers/yolo-auto.json")
        })
        .expect("custom provider change");
    let provider_after = provider_change.after.as_deref().unwrap();
    assert!(provider_after.contains("qwen3.8-27b"));
    assert!(provider_after.contains("yolo-auto.example"));
    let config_change = native
        .changes
        .iter()
        .find(|change| change.file_path.ends_with("config.yaml"))
        .expect("keyring mode config change");
    assert!(
        config_change
            .after
            .as_deref()
            .unwrap()
            .contains("GOOSE_DISABLE_KEYRING: true")
    );
    assert_eq!(native.protected_changes.len(), 1);
    assert!(matches!(
        native.protected_changes[0].target,
        chm_harness_sdk::adapter::protected::ProtectedTarget::GooseSecretsFile { .. }
    ));
}

#[test]
fn roo_mcp_file_is_read_without_provider_profile_warning() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("mcp.json");
    std::fs::write(
        &config,
        r#"{"mcpServers":{"docs":{"url":"https://mcp.example/sse"}}}"#,
    )
    .unwrap();
    let state = RooAdapter
        .read_state(&install("roo-code", &config.display().to_string()))
        .unwrap();
    assert_eq!(state.mcp.len(), 1);
    assert!(state.warnings.is_empty());
}

#[test]
fn kimi_writes_mcp_to_the_documented_sibling_json_file() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    let mcp = dir.path().join("mcp.json");
    std::fs::write(
        &config,
        "[providers.openai]\nbase_url = \"https://api.example/v1\"\n",
    )
    .unwrap();
    std::fs::write(&mcp, r#"{"mcpServers":{"old":{"command":"old"}}}"#).unwrap();
    let server = chm_core::domain::mcp::McpServer {
        id: uuid::Uuid::new_v4(),
        name: "docs".into(),
        transport: chm_core::domain::mcp::McpTransport::Http,
        command: None,
        args: vec![],
        url: Some("https://mcp.example/mcp".into()),
        env: serde_json::Map::new(),
        scope_type: chm_core::domain::mcp::ScopeType::Global,
        scope_path: None,
        provenance: serde_json::json!({"source":"test"}),
        enabled: true,
    };
    let native = KimiAdapter
        .plan(
            &ReconciliationPlan {
                actions: vec![PlanAction::Add(AddAction {
                    kind: "mcp".into(),
                    identity: "docs".into(),
                    payload: serde_json::to_value(server).unwrap(),
                    native_provider_id: None,
                })],
            },
            &install("kimi-cli", &config.display().to_string()),
        )
        .unwrap();
    assert_eq!(native.changes.len(), 1);
    assert_eq!(native.changes[0].file_path, mcp.display().to_string());
    let after = native.changes[0].after.as_deref().unwrap();
    assert!(after.contains("\"docs\""));
    assert!(after.contains("\"url\": \"https://mcp.example/mcp\""));
    assert!(!after.contains("mcp_servers"));
}

#[test]
fn kimi_treats_bare_url_as_http_and_explicit_sse_as_legacy_sse() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    let mcp = dir.path().join("mcp.json");
    std::fs::write(
        &config,
        "[providers.openai]\nbase_url = \"https://api.example/v1\"\n",
    )
    .unwrap();
    std::fs::write(
        &mcp,
        r#"{"mcpServers":{"http":{"url":"https://mcp.example/mcp"},"sse":{"transport":"sse","url":"https://mcp.example/sse"}}}"#,
    )
    .unwrap();
    let state = KimiAdapter
        .read_state(&install("kimi-cli", &config.display().to_string()))
        .unwrap();
    let http = state
        .mcp
        .iter()
        .find(|server| server.native_name == "http")
        .unwrap();
    let sse = state
        .mcp
        .iter()
        .find(|server| server.native_name == "sse")
        .unwrap();
    assert_eq!(
        http.server.transport,
        chm_core::domain::mcp::McpTransport::Http
    );
    assert_eq!(
        sse.server.transport,
        chm_core::domain::mcp::McpTransport::Sse
    );
}

#[test]
fn qwen_writes_streamable_http_using_http_url() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("settings.json");
    std::fs::write(&config, r#"{"mcpServers":{}}"#).unwrap();
    let server = chm_core::domain::mcp::McpServer {
        id: uuid::Uuid::new_v4(),
        name: "remote".into(),
        transport: chm_core::domain::mcp::McpTransport::Http,
        command: None,
        args: vec![],
        url: Some("https://mcp.example/mcp".into()),
        env: serde_json::Map::new(),
        scope_type: chm_core::domain::mcp::ScopeType::Global,
        scope_path: None,
        provenance: serde_json::json!({"source":"test"}),
        enabled: true,
    };
    let native = QwenAdapter
        .plan(
            &ReconciliationPlan {
                actions: vec![PlanAction::Add(AddAction {
                    kind: "mcp".into(),
                    identity: "remote".into(),
                    payload: serde_json::to_value(server).unwrap(),
                    native_provider_id: None,
                })],
            },
            &install("qwen-code", &config.display().to_string()),
        )
        .unwrap();
    let after = native.changes[0].after.as_deref().unwrap();
    assert!(after.contains("\"httpUrl\": \"https://mcp.example/mcp\""));
    assert!(!after.contains("\"url\": \"https://mcp.example/mcp\""));
}

#[test]
fn amp_url_only_mcp_defaults_to_modern_http_and_explicit_sse_is_preserved() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("settings.json");
    std::fs::write(
        &config,
        r#"{"amp.mcpServers":{"modern":{"url":"https://mcp.example/mcp"},"legacy":{"url":"https://mcp.example/sse","transport":"sse"}}}"#,
    )
    .unwrap();
    let state = AmpAdapter
        .read_state(&install("amp", &config.display().to_string()))
        .unwrap();
    assert_eq!(state.mcp.len(), 2);
    assert_eq!(
        state
            .mcp
            .iter()
            .find(|server| server.native_name == "modern")
            .unwrap()
            .server
            .transport,
        chm_core::domain::mcp::McpTransport::Http
    );
    assert_eq!(
        state
            .mcp
            .iter()
            .find(|server| server.native_name == "legacy")
            .unwrap()
            .server
            .transport,
        chm_core::domain::mcp::McpTransport::Sse
    );
}

#[test]
fn cline_recognizes_camel_case_streamable_http_and_disabled_servers() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("cline_mcp_settings.json");
    std::fs::write(
        &config,
        r#"{"mcpServers":{"remote":{"type":"streamableHttp","url":"https://mcp.example/mcp","disabled":true}}}"#,
    )
    .unwrap();
    let state = ClineAdapter
        .read_state(&install("cline", &config.display().to_string()))
        .unwrap();
    assert_eq!(state.mcp.len(), 1);
    assert_eq!(
        state.mcp[0].server.transport,
        chm_core::domain::mcp::McpTransport::Http
    );
    assert!(!state.mcp[0].server.enabled);

    let native = ClineAdapter
        .plan(
            &ReconciliationPlan {
                actions: vec![PlanAction::Add(AddAction {
                    kind: "mcp".into(),
                    identity: "new-remote".into(),
                    payload: serde_json::json!({
                        "id": uuid::Uuid::new_v4(),
                        "name": "new-remote",
                        "transport": "http",
                        "command": null,
                        "args": [],
                        "url": "https://mcp.example/new",
                        "env": {},
                        "scope_type": "global",
                        "scope_path": null,
                        "provenance": {},
                        "enabled": true
                    }),
                    native_provider_id: None,
                })],
            },
            &install("cline", &config.display().to_string()),
        )
        .unwrap();
    let after = native.changes[0].after.as_deref().unwrap();
    assert!(after.contains(r#""type": "streamableHttp""#));
    assert!(!after.contains(r#""type": "streamable-http""#));
}

#[test]
fn cline_reads_current_provider_scoped_model_map_and_preserves_shape_on_write() {
    let dir = tempfile::tempdir().unwrap();
    let settings = dir.path().join(".cline/data/settings");
    std::fs::create_dir_all(&settings).unwrap();
    let providers = settings.join("providers.json");
    let models = settings.join("models.json");
    std::fs::write(&providers, r#"{"openai-compatible":{"provider":"openai-compatible","baseUrl":"https://llm.example/v1"}}"#).unwrap();
    std::fs::write(
        &models,
        r#"{"providers":{"openai-compatible":{"models":{"qwen3":{"name":"Qwen 3","contextWindow":32768}}}}}"#,
    )
    .unwrap();
    let state = ClineAdapter
        .read_state(&install("cline", &providers.display().to_string()))
        .unwrap();
    assert_eq!(state.models.len(), 1);
    assert_eq!(state.models[0].native_id, "qwen3");
    assert_eq!(state.models[0].route.context_window, Some(32768));

    let wrapped_providers = settings.join("wrapped-providers.json");
    std::fs::write(
        &wrapped_providers,
        r#"{"lastUsedProvider":"openai-compatible","providers":{"openai-compatible":{"settings":{"provider":"openai-compatible","model":"qwen3","apiKey":"secret"},"updatedAt":123,"tokenSource":"apiKey"}}}"#,
    )
    .unwrap();
    let wrapped_state = ClineAdapter
        .read_state(&install("cline", &wrapped_providers.display().to_string()))
        .unwrap();
    assert_eq!(wrapped_state.providers.len(), 1);
    assert_eq!(
        wrapped_state.providers[0]["native_provider_id"],
        "openai-compatible"
    );
    assert_eq!(wrapped_state.providers[0]["model"], "qwen3");
    assert_eq!(wrapped_state.profiles[0]["provider"], "openai-compatible");
    assert_eq!(wrapped_state.profiles[0]["model"], "qwen3");
    assert!(wrapped_state.providers[0].get("apiKey").is_none());

    let native = ClineAdapter
        .plan(
            &ReconciliationPlan {
                actions: vec![PlanAction::Update(UpdateAction {
                    kind: "model".into(),
                    identity: "qwen3".into(),
                    changed_fields: vec!["display_name".into(), "context_window".into()],
                    desired: serde_json::json!({"display_name":"Qwen 3 tuned", "context_window":65536}),
                    current: serde_json::json!({}),
                    native_provider_id: Some("openai-compatible".into()),
                })],
            },
            &install("cline", &providers.display().to_string()),
        )
        .unwrap();
    assert_eq!(native.changes.len(), 1);
    let after = native.changes[0].after.as_deref().unwrap();
    assert!(after.contains("\"providers\""));
    assert!(after.contains("Qwen 3 tuned"));
    assert!(after.contains("\"contextWindow\": 65536"));
}

#[test]
fn cline_new_model_catalog_uses_current_wrapper_and_legacy_arrays_keep_ids() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("providers.json");
    std::fs::write(&config, r#"{}"#).unwrap();
    let action = PlanAction::Add(AddAction {
        kind: "model".into(),
        identity: "qwen3".into(),
        payload: serde_json::json!({
            "native_provider_id": "openai-compatible",
            "display_name": "Qwen 3",
            "context_window": 32768,
            "max_output": 8192
        }),
        native_provider_id: Some("openai-compatible".into()),
    });
    let native = ClineAdapter
        .plan(
            &ReconciliationPlan {
                actions: vec![action],
            },
            &install("cline", &config.display().to_string()),
        )
        .unwrap();
    let after = native.changes[0].after.as_deref().unwrap();
    assert!(after.contains("\"providers\""));
    assert!(after.contains("\"openai-compatible\""));
    assert!(after.contains("\"qwen3\""));
    assert!(after.contains("\"contextWindow\": 32768"));
    assert!(after.contains("\"maxTokens\": 8192"));

    let legacy_root = tempfile::tempdir().unwrap();
    let legacy_settings = legacy_root.path().join(".cline/data/settings");
    std::fs::create_dir_all(&legacy_settings).unwrap();
    let legacy_config = legacy_root.path().join("providers.json");
    std::fs::write(&legacy_config, r#"{}"#).unwrap();
    let legacy = legacy_settings.join("models.json");
    std::fs::write(
        &legacy,
        r#"{"openai-compatible":{"models":[{"id":"old","name":"Old"}]}}"#,
    )
    .unwrap();
    let native = ClineAdapter
        .plan(
            &ReconciliationPlan {
                actions: vec![PlanAction::Add(AddAction {
                    kind: "model".into(),
                    identity: "new".into(),
                    payload: serde_json::json!({
                        "native_provider_id": "openai-compatible",
                        "display_name": "New"
                    }),
                    native_provider_id: Some("openai-compatible".into()),
                })],
            },
            &install("cline", &legacy_config.display().to_string()),
        )
        .unwrap();
    let after = native.changes[0].after.as_deref().unwrap();
    assert!(after.contains("\"id\": \"new\""));
}

#[test]
fn goose_reads_current_provider_map_and_active_selection() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.yaml");
    std::fs::write(
        &config,
        r#"active_provider: local
providers:
  local:
    display_name: Local gateway
    model: qwen3-coder
    base_url: https://llm.example/v1
    api_key_env: LOCAL_KEY
"#,
    )
    .unwrap();
    let state = GooseAdapter
        .read_state(&install("goose", &config.display().to_string()))
        .unwrap();
    assert_eq!(state.providers.len(), 1);
    assert_eq!(state.providers[0]["native_provider_id"], "local");
    assert_eq!(state.profiles[0]["provider"], "local");
    assert_eq!(state.profiles[0]["model"], "qwen3-coder");
    assert_eq!(state.providers[0]["api_key_env"], "<redacted>");
}
