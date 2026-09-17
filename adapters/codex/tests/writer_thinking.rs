//! Unit tests for `codex_adapter::writer::{reasoning_effort_for_levels,
//! set_reasoning_effort}`. Codex stores a single `model_reasoning_effort` per
//! profile file rather than a per-model map, so CHM collapses the route's
//! selected thinking levels to one OpenAI-compatible value
//! (`minimal|low|medium|high`); CHM-only levels (`xhigh|max`) fall back to
//! `high`.

use codex_adapter::writer::{reasoning_effort_for_levels, set_reasoning_effort};

fn level(s: &str) -> String {
    s.to_string()
}

#[test]
fn picks_highest_openai_level() {
    assert_eq!(
        reasoning_effort_for_levels(&[level("low"), level("medium"), level("high")]),
        Some("high")
    );
    assert_eq!(
        reasoning_effort_for_levels(&[level("medium"), level("low")]),
        Some("medium")
    );
    assert_eq!(
        reasoning_effort_for_levels(&[level("low"), level("minimal")]),
        Some("low")
    );
}

#[test]
fn xhigh_and_max_collapse_to_high_when_no_openai_levels_present() {
    // When only CHM-only levels are selected, fall back to "high" so the file
    // still gets a usable value.
    assert_eq!(reasoning_effort_for_levels(&[level("xhigh")]), Some("high"));
    assert_eq!(reasoning_effort_for_levels(&[level("max")]), Some("high"));
}

#[test]
fn xhigh_does_not_outrank_explicit_openai_levels() {
    // If the user selected both standard openai levels and xhigh/max, the
    // standard level still wins so the upstream value matches what was asked.
    assert_eq!(
        reasoning_effort_for_levels(&[level("medium"), level("xhigh")]),
        Some("medium")
    );
}

#[test]
fn empty_or_unknown_levels_produce_none() {
    assert_eq!(reasoning_effort_for_levels(&[]), None);
    assert_eq!(reasoning_effort_for_levels(&[level("off")]), None);
    assert_eq!(reasoning_effort_for_levels(&[level("unsupported")]), None);
}

#[test]
fn set_reasoning_effort_writes_and_clears() {
    let raw = r#"model = "zai/glm-5"
model_provider = "zai"
"#;
    let mut doc: toml_edit::DocumentMut = raw.parse().unwrap();
    set_reasoning_effort(&mut doc, Some("high"));
    assert_eq!(
        doc["model_reasoning_effort"].as_str(),
        Some("high"),
        "effort must be written at the profile root"
    );

    set_reasoning_effort(&mut doc, None);
    assert!(
        doc.get("model_reasoning_effort").is_none(),
        "passing None must remove the key so Codex reverts to no-reasoning"
    );
}
