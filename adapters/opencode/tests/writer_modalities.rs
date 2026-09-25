//! Unit tests for OpenCode's modality handling in
//! `opencode_adapter::writer::fold_model`.
//!
//! OpenCode declares modalities as a native block on the model entry:
//!
//! ```json
//! "modalities": { "input": ["text", "image"], "output": ["text"] }
//! ```
//!
//! CHM stores its own canonical `input_modalities` capability, so the writer
//! has to translate between the two without leaking CHM's key into OpenCode's
//! config and without dropping output modalities it does not track.

use opencode_adapter::writer::fold_model;
use serde_json::{Value, json};

fn doc_with_model(model_id: &str) -> Value {
    let mut models = serde_json::Map::new();
    models.insert(model_id.to_string(), json!({ "name": model_id }));
    json!({ "provider": { "cline-pass": { "models": models } } })
}

#[test]
fn chm_declaration_becomes_the_native_block() {
    let mut doc = doc_with_model("deepseek-v4.1-flash");
    fold_model(
        &mut doc,
        "cline-pass",
        "deepseek-v4.1-flash",
        "DeepSeek V4.1 Flash",
        None,
        &json!({"input_modalities": ["text", "image"]}),
    );
    let model = &doc["provider"]["cline-pass"]["models"]["deepseek-v4.1-flash"];
    assert_eq!(
        model["modalities"],
        json!({"input": ["text", "image"], "output": ["text"]})
    );
}

#[test]
fn chm_key_does_not_leak_as_a_flat_field() {
    let mut doc = doc_with_model("deepseek-v4.1-flash");
    fold_model(
        &mut doc,
        "cline-pass",
        "deepseek-v4.1-flash",
        "DeepSeek V4.1 Flash",
        None,
        &json!({"input_modalities": ["text", "image", "video"]}),
    );
    let model = &doc["provider"]["cline-pass"]["models"]["deepseek-v4.1-flash"];
    assert!(
        model.get("input_modalities").is_none(),
        "CHM's canonical key must not appear in OpenCode's config: {model}"
    );
    // OpenCode natively understands video, so unlike Pi nothing is narrowed.
    assert_eq!(
        model["modalities"]["input"],
        json!(["text", "image", "video"])
    );
}

#[test]
fn declared_output_modalities_survive_the_translation() {
    let mut doc = doc_with_model("nano-banana");
    fold_model(
        &mut doc,
        "cline-pass",
        "nano-banana",
        "Nano Banana",
        None,
        &json!({
            "input_modalities": ["text", "image"],
            "modalities": {"input": ["text"], "output": ["text", "image"]}
        }),
    );
    let model = &doc["provider"]["cline-pass"]["models"]["nano-banana"];
    assert_eq!(
        model["modalities"],
        json!({"input": ["text", "image"], "output": ["text", "image"]}),
        "input follows CHM, output (which CHM does not track) is preserved"
    );
}

#[test]
fn without_a_declaration_native_modalities_pass_through_untouched() {
    let mut doc = json!({
        "provider": {
            "cline-pass": {
                "models": {
                    "from-opencode": {
                        "name": "From OpenCode",
                        "modalities": {"input": ["text", "image"], "output": ["text"]}
                    }
                }
            }
        }
    });
    fold_model(
        &mut doc,
        "cline-pass",
        "from-opencode",
        "From OpenCode",
        None,
        &json!({"modalities": {"input": ["text", "image"], "output": ["text"]}}),
    );
    let model = &doc["provider"]["cline-pass"]["models"]["from-opencode"];
    assert_eq!(
        model["modalities"],
        json!({"input": ["text", "image"], "output": ["text"]})
    );
}
