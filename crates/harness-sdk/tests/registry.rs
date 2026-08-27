use chm_harness_sdk::definition::{all_definitions, detection_only_definitions, tier1_definitions};

#[test]
fn tier1_has_exactly_five_harnesses() {
    let defs = tier1_definitions();
    assert_eq!(defs.len(), 5);
    let ids: Vec<&str> = defs.iter().map(|d| d.id).collect();
    assert!(ids.contains(&"claude-code"));
    assert!(ids.contains(&"codex"));
    assert!(ids.contains(&"opencode"));
    assert!(ids.contains(&"pi"));
    assert!(ids.contains(&"reasonix"));
    assert!(defs.iter().all(|d| !d.detection_only));
}

#[test]
fn every_definition_has_at_least_one_executable_name() {
    for d in all_definitions() {
        assert!(
            !d.executable_names.is_empty(),
            "{} has no executables",
            d.id
        );
    }
}

#[test]
fn tier1_ids_match_domain_harness_types() {
    let defs = tier1_definitions();
    for d in &defs {
        assert_eq!(
            chm_core::domain::harness::HarnessType::parse_str(d.id).as_str(),
            d.id
        );
    }
}

#[test]
fn detection_only_has_ten_entries() {
    assert_eq!(detection_only_definitions().len(), 10);
}
