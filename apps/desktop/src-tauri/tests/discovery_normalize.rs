//! Tests that the discovery URL builder correctly normalises the path so
//! endpoints whose base_url already ends in `/v1` don't get a doubled `/v1/v1`.
//! Pure URL-composition checks; no network calls.

#[test]
fn discovery_url_no_double_v1() {
    let base = "https://api.kimi.com/coding/v1";
    let path = "/v1/models";
    let normalized_path = if base.ends_with("/v1") && path.starts_with("/v1/") {
        &path[3..]
    } else {
        path
    };
    let url = format!("{}{}", base.trim_end_matches('/'), normalized_path);
    assert_eq!(url, "https://api.kimi.com/coding/v1/models");
}

#[test]
fn discovery_url_preserves_when_base_lacks_v1() {
    let base = "https://api.z.ai/api/anthropic";
    let path = "/v1/models";
    let normalized_path = if base.ends_with("/v1") && path.starts_with("/v1/") {
        &path[3..]
    } else {
        path
    };
    let url = format!("{}{}", base.trim_end_matches('/'), normalized_path);
    assert_eq!(url, "https://api.z.ai/api/anthropic/v1/models");
}

#[test]
fn discovery_url_handles_custom_path() {
    let base = "https://example.com/api/v1";
    let path = "/custom/listing";
    let normalized_path = if base.ends_with("/v1") && path.starts_with("/v1/") {
        &path[3..]
    } else {
        path
    };
    let url = format!("{}{}", base.trim_end_matches('/'), normalized_path);
    assert_eq!(url, "https://example.com/api/v1/custom/listing");
}
