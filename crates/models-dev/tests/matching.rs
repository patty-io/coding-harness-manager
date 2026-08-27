use chm_models_dev::{ModelsDevCatalog, match_model};
use serde_json::Value;

fn load_catalog() -> ModelsDevCatalog {
    let raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/catalog.json"
    ))
    .unwrap();
    let parsed: Value = serde_json::from_str(&raw).unwrap();
    // api.json shape: map of provider -> { models: { id: {...} } }
    let mut models = Vec::new();
    if let Some(providers) = parsed.as_object() {
        for (_provider, pv) in providers {
            if let Some(map) = pv.get("models").and_then(|m| m.as_object()) {
                for (id, meta) in map {
                    models.push(chm_models_dev::ModelsDevModel {
                        id: id.clone(),
                        name: meta
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or(id)
                            .to_string(),
                        provider: pv.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()),
                        context_window: meta.get("context").and_then(|v| v.as_i64()),
                        max_output: meta.get("max_output").and_then(|v| v.as_i64()),
                        modalities: meta.clone(),
                    });
                }
            }
        }
    }
    ModelsDevCatalog { models }
}

#[test]
fn exact_id_is_100_confidence() {
    let catalog = load_catalog();
    let hit = match_model("gpt-4o", &catalog);
    assert_eq!(
        hit.confidence, 100,
        "gpt-4o should exist in the live fixture"
    );
    assert!(hit.model.is_some());
}

#[test]
fn openrouter_prefixed_id_matches_known_alias() {
    let catalog = load_catalog();
    let hit = match_model("openai/gpt-4o", &catalog);
    assert!(
        hit.confidence >= 95,
        "provider-prefixed id should match at alias level"
    );
    assert!(hit.model.is_some());
}

#[test]
fn garbage_id_is_unknown() {
    let catalog = load_catalog();
    assert_eq!(
        match_model("totally-not-a-real-model-xyz", &catalog).confidence,
        0
    );
}

#[test]
fn normalized_id_matches_85() {
    let catalog = load_catalog();
    // e.g. "GPT-4O" (uppercase) normalizes to the same token sequence as gpt-4o
    let hit = match_model("GPT-4O", &catalog);
    assert!(hit.confidence >= 85, "expected normalized match");
    assert!(hit.model.is_some());
}
