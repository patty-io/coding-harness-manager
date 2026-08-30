use adapters::all_adapters;
use chm_harness_sdk::definition::all_definitions;

#[test]
fn registry_contains_all_supported_adapters() {
    let adapters = all_adapters();
    let ids: Vec<&str> = adapters.iter().map(|a| a.id()).collect();
    for expected in [
        "claude-code",
        "codex",
        "opencode",
        "pi",
        "reasonix",
        "gemini-cli",
        "qwen-code",
        "kimi-cli",
        "cursor",
        "cline",
        "roo-code",
        "aider",
        "amp",
        "goose",
        "continue",
    ] {
        assert!(ids.contains(&expected), "missing {expected}");
    }
}

#[test]
fn every_definition_has_a_registered_adapter() {
    let adapters = all_adapters();
    let ids: Vec<&str> = adapters.iter().map(|adapter| adapter.id()).collect();
    for definition in all_definitions() {
        assert!(
            ids.contains(&definition.id),
            "definition {} has no registered adapter",
            definition.id
        );
    }
}

#[test]
fn every_adapter_capabilities_are_sane() {
    for a in all_adapters() {
        let caps = a.capabilities();
        assert!(
            caps.supports_custom_models
                || caps.supports_mcp_global
                || caps.supports_global_skills
                || caps.supports_profiles
                || caps.supports_runtime_env,
            "{} declares no supported surface",
            a.id()
        );
    }
}
