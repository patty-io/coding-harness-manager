//! Canonical model-capability vocabulary shared by commands and adapters.
//!
//! Modalities follow the models.dev vocabulary (`text`, `image`, `audio`,
//! `video`, `pdf`). CHM keeps the full set on a route so a harness that can
//! express more (OpenCode) is not clipped, while harnesses that only know a
//! subset (Pi accepts `text` and `image`) filter on their side of the fence.

use serde_json::Value;

/// Input modalities CHM tracks, in canonical order.
pub const INPUT_MODALITIES: [&str; 5] = ["text", "image", "audio", "video", "pdf"];

/// Normalize a modality list: unknown values are dropped, ordering follows
/// [`INPUT_MODALITIES`], duplicates collapse, and `text` is always present
/// because every harness model accepts text.
pub fn canonicalize_input_modalities<I, S>(values: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let requested: Vec<String> = values
        .into_iter()
        .map(|value| value.as_ref().trim().to_ascii_lowercase())
        .collect();
    let mut normalized: Vec<String> = INPUT_MODALITIES
        .iter()
        .filter(|canonical| requested.iter().any(|asked| asked == *canonical))
        .map(|canonical| (*canonical).to_string())
        .collect();
    if !normalized.iter().any(|value| value == "text") {
        normalized.insert(0, "text".to_string());
    }
    normalized
}

/// Read `capabilities.input_modalities`.
///
/// Returns `None` when the route carries no declaration at all, which is the
/// signal for adapters to leave their native config untouched rather than
/// declaring a model text-only on CHM's behalf.
pub fn input_modalities(capabilities: &Value) -> Option<Vec<String>> {
    let entries = capabilities.get("input_modalities")?.as_array()?;
    let declared: Vec<&str> = entries.iter().filter_map(Value::as_str).collect();
    if declared.is_empty() {
        return None;
    }
    Some(canonicalize_input_modalities(declared))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unknown_values_are_dropped_and_text_is_always_kept() {
        assert_eq!(
            canonicalize_input_modalities(["IMAGE", "wat", "video"]),
            vec!["text", "image", "video"]
        );
        assert_eq!(
            canonicalize_input_modalities::<[&str; 0], &str>([]),
            vec!["text"]
        );
    }

    #[test]
    fn missing_declaration_reads_as_unknown_not_text_only() {
        assert_eq!(input_modalities(&json!({"reasoning": true})), None);
        assert_eq!(input_modalities(&json!({"input_modalities": []})), None);
        assert_eq!(
            input_modalities(&json!({"input_modalities": ["image"]})),
            Some(vec!["text".to_string(), "image".to_string()])
        );
    }
}
