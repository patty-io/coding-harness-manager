//! Unit tests for `opencode_adapter::writer::set_model_thinking`, pinning the
//! shape to OpenCode's variant format observed in the 1.18.23 fixture:
//!
//! ```json
//! "variants": {
//!     "high":   { "reasoningEffort": "high" },
//!     "medium": { "reasoningEffort": "medium" },
//!     "xhigh":  { "reasoningEffort": "xhigh" }
//! }
//! ```

use opencode_adapter::writer::set_model_thinking;
use serde_json::{Value, json};

fn doc_with_model(id: &str) -> Value {
    json!({
        "provider": {
            "cline-pass": {
                "models": {
                    id: {"name": id}
                }
            }
        }
    })
}

#[test]
fn writes_one_variant_per_listed_level() {
    let mut doc = doc_with_model("deepseek/deepseek-v4.1-flash");
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
    let model = &doc["provider"]["cline-pass"]["models"]["deepseek/deepseek-v4.1-flash"];
    let variants = model.get("variants").expect("variants block written");
    for level in ["medium", "high", "xhigh", "max"] {
        assert_eq!(variants[level]["reasoningEffort"], json!(level));
    }
    assert_eq!(variants.as_object().unwrap().len(), 4);
}

#[test]
fn clearing_reasoning_strips_variants() {
    let mut doc = doc_with_model("contributor/contributor-x");
    set_model_thinking(
        &mut doc,
        Some("cline-pass"),
        "contributor/contributor-x",
        Some(true),
        Some(&["high".to_string(), "xhigh".to_string(), "max".to_string()]),
    );
    assert!(
        doc["provider"]["cline-pass"]["models"]["contributor/contributor-x"]
            .get("variants")
            .is_some(),
        "variants written when reasoning is enabled"
    );

    set_model_thinking(
        &mut doc,
        Some("cline-pass"),
        "contributor/contributor-x",
        Some(false),
        None,
    );
    let model = &doc["provider"]["cline-pass"]["models"]["contributor/contributor-x"];
    assert!(
        model.get("variants").is_none(),
        "reasoning=false must remove the existing variants"
    );
}

#[test]
fn reasoning_capability_does_not_leak_as_flat_field() {
    // The writer's `fold_model_with_provider` copies capabilities wholesale
    // EXCEPT for `reasoning`/`thinking_levels`. This guards against those
    // keys leaking back into the model entry as flat keys.
    use opencode_adapter::writer::fold_model_with_provider;
    let mut doc = json!({"provider": {}});
    fold_model_with_provider(
        &mut doc,
        "cline-pass",
        "deepseek/deepseek-v4.1-flash",
        "DeepSeek V4.1 Flash",
        None,
        &json!({
            "reasoning": true,
            "thinking_levels": ["medium", "high"],
            "modalities": ["text"]
        }),
        None,
    );
    let model = &doc["provider"]["cline-pass"]["models"]["deepseek/deepseek-v4.1-flash"];
    assert!(
        model.get("reasoning").is_none(),
        "reasoning must not leak as a flat model field"
    );
    assert!(
        model.get("thinking_levels").is_none(),
        "thinking_levels must not leak as a flat model field"
    );
    assert_eq!(model["modalities"], json!(["text"]));
}

#[test]
fn no_declaration_leaves_existing_variants_untouched() {
    let mut doc = json!({
        "provider": {
            "cline-pass": {
                "models": {
                    "no-touch/model": {
                        "variants": {"high": {"reasoningEffort": "high"}}
                    }
                }
            }
        }
    });
    set_model_thinking(&mut doc, Some("cline-pass"), "no-touch/model", None, None);
    let variants = &doc["provider"]["cline-pass"]["models"]["no-touch/model"]["variants"];
    assert_eq!(variants["high"]["reasoningEffort"], json!("high"));
}
