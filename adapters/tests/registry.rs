use adapters::all_adapters;

#[test]
fn registry_contains_all_five_tier1_adapters() {
    let adapters = all_adapters();
    let ids: Vec<&str> = adapters.iter().map(|a| a.id()).collect();
    for expected in ["claude-code", "codex", "opencode", "pi", "reasonix"] {
        assert!(ids.contains(&expected), "missing {expected}");
    }
}

#[test]
fn every_adapter_capabilities_are_sane() {
    for a in all_adapters() {
        let caps = a.capabilities();
        assert!(
            caps.supports_custom_models || caps.supports_mcp_global || caps.supports_global_skills,
            "{} declares no supported surface",
            a.id()
        );
    }
}
