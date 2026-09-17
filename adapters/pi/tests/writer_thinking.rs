//! Unit tests for `adapters_pi::writer::set_model_thinking`, pinning the
//! output shape to what pi's runtime (`getSupportedThinkingLevels` in
//! `dist/bundle/chunks/chunk-IDDQWTHI.js`) accepts:
//!
//! * `reasoning: true` is required to expose any thinking level.
//! * Listed levels map to their own provider value; every other canonical
//!   level is explicitly `null` so pi hides it.
//! * `reasoning: false` also strips any existing `thinkingLevelMap`.
//! * `None` with no levels leaves the entry untouched.
//!
//! These tests run as a separate crate test so they exercise the writer
//! directly without standing up the full sync stack.

use pi_adapter::writer::{THINKING_LEVELS, set_model_thinking};
use serde_json::{Value, json};

fn fresh_doc() -> Value {
    json!({
        "providers": {
            "cline-pass": {
                "baseUrl": "https://api.cline.bot/api/v1",
                "models": [
                    {"id": "deepseek/deepseek-v4.1-flash"},
                    {"id": "contributor/contributor-x"},
                    {"id": "no-touch/model"}
                ]
            }
        }
    })
}

#[test]
fn every_canonical_level_is_pinned_either_supported_or_null() {
    let mut doc = fresh_doc();
    set_model_thinking(
        &mut doc,
        Some("cline-pass"),
        "deepseek/deepseek-v4.1-flash",
        Some(true),
        Some(&[
            "medium".to_string(),
            "high".to_string(),
            "xhigh".to_string(),
            "max".to_string(),
        ]),
    );
    let map = &doc["providers"]["cline-pass"]["models"][0]["thinkingLevelMap"];
    for level in THINKING_LEVELS {
        let value = &map[level];
        assert!(
            value.is_null() || value.is_string(),
            "level {level} must be either string (supported) or null (hidden), got {value}"
        );
    }
}

#[test]
fn contributor_model_only_exposes_high_xhigh_max() {
    let mut doc = fresh_doc();
    set_model_thinking(
        &mut doc,
        Some("cline-pass"),
        "contributor/contributor-x",
        Some(true),
        Some(&["high".to_string(), "xhigh".to_string(), "max".to_string()]),
    );
    let map = &doc["providers"]["cline-pass"]["models"][1]["thinkingLevelMap"];
    assert_eq!(map["high"], json!("high"));
    assert_eq!(map["xhigh"], json!("xhigh"));
    assert_eq!(map["max"], json!("max"));
    assert_eq!(map["medium"], Value::Null);
    assert_eq!(map["low"], Value::Null);
    assert_eq!(map["minimal"], Value::Null);
    assert_eq!(map["off"], Value::Null);
}

#[test]
fn clearing_reasoning_strips_the_map() {
    let mut doc = fresh_doc();
    // First, enable thinking.
    set_model_thinking(
        &mut doc,
        Some("cline-pass"),
        "deepseek/deepseek-v4.1-flash",
        Some(true),
        Some(&["high".to_string()]),
    );
    assert!(
        doc["providers"]["cline-pass"]["models"][0]
            .get("thinkingLevelMap")
            .is_some(),
        "map should be written when reasoning is enabled"
    );

    // Then, disable it.
    set_model_thinking(
        &mut doc,
        Some("cline-pass"),
        "deepseek/deepseek-v4.1-flash",
        Some(false),
        None,
    );
    let model = &doc["providers"]["cline-pass"]["models"][0];
    assert_eq!(model["reasoning"], json!(false));
    assert!(
        model.get("thinkingLevelMap").is_none(),
        "reasoning=false must remove the existing map"
    );
}

#[test]
fn no_declaration_leaves_existing_config_untouched() {
    let mut doc = fresh_doc();
    // Pre-seed a thinking config on the untouched model.
    doc["providers"]["cline-pass"]["models"][2]["reasoning"] = json!(true);
    doc["providers"]["cline-pass"]["models"][2]["thinkingLevelMap"] = json!({
        "off": null, "minimal": null, "low": null, "medium": null,
        "high": "high", "xhigh": null, "max": null,
    });

    // A no-op call (reasoning=None, levels=None) must not touch this entry.
    set_model_thinking(&mut doc, Some("cline-pass"), "no-touch/model", None, None);
    let model = &doc["providers"]["cline-pass"]["models"][2];
    assert_eq!(model["reasoning"], json!(true));
    assert!(model.get("thinkingLevelMap").is_some());
}
