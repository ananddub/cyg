use super::*;
use crate::error::INVALID_ARGUMENT;

fn body(value: serde_json::Value) -> UpdateGameRequest {
    serde_json::from_value(value).expect("update body")
}

fn mask(fields: &[&str]) -> Vec<String> {
    fields.iter().map(|field| (*field).to_owned()).collect()
}

#[test]
fn an_explicit_mask_updates_only_the_fields_it_names() {
    let request = body(json!({
        "name": "Renamed",
        "status": "suspended",
        "tick_rate": 20,
    }));
    let filtered = masked_update(&request, Some(&mask(&["name"]))).expect("mask applies");
    assert_eq!(filtered.name.as_deref(), Some("Renamed"));
    assert_eq!(filtered.status, None, "a field outside the mask is ignored");
    assert_eq!(filtered.tick_rate, None, "and so is the flattened config");
}

#[test]
fn a_mask_that_names_a_field_the_body_does_not_set_is_refused() {
    let request = body(json!({"name": "Renamed"}));
    let err = masked_update(&request, Some(&mask(&["status"]))).expect_err("missing value");
    assert_eq!(err.kind, INVALID_ARGUMENT, "{err:?}");
    assert!(err.message.contains("status"), "{err:?}");
}

#[test]
fn a_flat_config_mask_reads_the_nested_config_value() {
    let request = body(json!({"config": {"tick_rate": 90}}));
    let filtered =
        masked_update(&request, Some(&mask(&["config.tick_rate"]))).expect("nested mask applies");
    assert_eq!(
        filtered.tick_rate,
        Some(90),
        "the flat spelling is a view of the same config key"
    );
}

#[test]
fn an_empty_or_unknown_mask_is_refused() {
    let request = body(json!({"name": "Renamed"}));
    let empty = masked_update(&request, Some(&[])).expect_err("empty mask");
    assert_eq!(empty.kind, INVALID_ARGUMENT, "{empty:?}");
    let unknown = masked_update(&request, Some(&mask(&["owner"]))).expect_err("unknown field");
    assert_eq!(unknown.kind, INVALID_ARGUMENT, "{unknown:?}");
    assert!(unknown.message.contains("owner"), "{unknown:?}");
}
