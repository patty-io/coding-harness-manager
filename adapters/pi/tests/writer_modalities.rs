//! Unit tests for `pi_adapter::writer::set_model_input`, pinning the shape
//! Pi's models.json schema accepts for per-model modality declarations.
//!
//! * Pi's `input` field only admits `text` and `image`, and Pi validates the
//!   whole file — a wider value would make it drop every custom provider.
//! * A declaration always ends up containing `text`.
//! * `None` (CHM knows nothing) must not stamp the model text-only, so a
//!   hand-written `input` survives a sync.
//!
//! These run as a separate crate test so they exercise the writer directly.

use pi_adapter::writer::set_model_input;
use serde_json::{Value, json};

fn fresh_doc() -> Value {
    json!({
        "providers": {
            "patty-omni": {
                "baseUrl": "https://omni.patty.io/v1",
                "models": [
                    {"id": "opencode-go/deepseek-v4.1-flash"},
                    {"id": "hand-written/model", "input": ["text", "image"]}
                ]
            }
        }
    })
}

fn input_of(doc: &Value, model_id: &str) -> Option<Value> {
    doc["providers"]["patty-omni"]["models"]
        .as_array()?
        .iter()
        .find(|model| model["id"].as_str() == Some(model_id))
        .map(|model| model["input"].clone())
}

#[test]
fn image_declaration_becomes_pi_input_array() {
    let mut doc = fresh_doc();
    let found = set_model_input(
        &mut doc,
        Some("patty-omni"),
        "opencode-go/deepseek-v4.1-flash",
        Some(&["text".to_string(), "image".to_string()]),
    );
    assert!(found, "model must be located and updated");
    assert_eq!(
        input_of(&doc, "opencode-go/deepseek-v4.1-flash"),
        Some(json!(["text", "image"]))
    );
}

#[test]
fn wider_modalities_are_narrowed_to_what_pi_can_express() {
    let mut doc = fresh_doc();
    // audio/video/pdf would fail Pi's schema and take the whole file down.
    set_model_input(
        &mut doc,
        Some("patty-omni"),
        "opencode-go/deepseek-v4.1-flash",
        Some(&[
            "video".to_string(),
            "audio".to_string(),
            "pdf".to_string(),
            "image".to_string(),
        ]),
    );
    assert_eq!(
        input_of(&doc, "opencode-go/deepseek-v4.1-flash"),
        Some(json!(["text", "image"]))
    );
}

#[test]
fn a_declaration_without_text_still_keeps_text() {
    let mut doc = fresh_doc();
    set_model_input(
        &mut doc,
        Some("patty-omni"),
        "opencode-go/deepseek-v4.1-flash",
        Some(&["image".to_string()]),
    );
    assert_eq!(
        input_of(&doc, "opencode-go/deepseek-v4.1-flash"),
        Some(json!(["text", "image"]))
    );
}

#[test]
fn no_declaration_leaves_a_hand_written_input_untouched() {
    let mut doc = fresh_doc();
    let found = set_model_input(&mut doc, Some("patty-omni"), "hand-written/model", None);
    assert!(!found, "nothing to write means nothing was touched");
    assert_eq!(
        input_of(&doc, "hand-written/model"),
        Some(json!(["text", "image"])),
        "CHM must not declare a model text-only on the user's behalf"
    );
}

#[test]
fn provider_scope_keeps_sibling_providers_intact() {
    let mut doc = json!({
        "providers": {
            "patty-omni": {"models": [{"id": "shared/id"}]},
            "omniroute": {"models": [{"id": "shared/id"}]}
        }
    });
    set_model_input(
        &mut doc,
        Some("patty-omni"),
        "shared/id",
        Some(&["text".to_string(), "image".to_string()]),
    );
    assert_eq!(
        doc["providers"]["patty-omni"]["models"][0]["input"],
        json!(["text", "image"])
    );
    assert!(
        doc["providers"]["omniroute"]["models"][0]
            .get("input")
            .is_none(),
        "sibling providers must not be modified"
    );
}
