use chm_harness_sdk::adapter::types::{HarnessCapabilities, parse_version_supported};

#[test]
fn capabilities_default_to_safe_false_then_opt_in() {
    let caps = HarnessCapabilities::none();
    assert!(!caps.supports_custom_models);
    let caps = caps.with_models(true);
    assert!(caps.supports_custom_models);
}

#[test]
fn version_support_matches_prefix() {
    assert!(parse_version_supported(Some("0.30.2"), &["0.30"]));
    assert!(parse_version_supported(Some("0.31.0"), &["0.31"]));
    assert!(!parse_version_supported(Some("1.2.0"), &["0.31"]));
    assert!(parse_version_supported(None, &["0.31"]));
}
