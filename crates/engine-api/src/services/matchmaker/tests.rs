//! Unit tests for the query term language.

use std::collections::HashMap;

use super::query_matches;

fn props() -> (HashMap<String, String>, HashMap<String, f64>) {
    (
        HashMap::from([("mode".to_string(), "deathmatch".to_string())]),
        HashMap::from([("rank".to_string(), 42.0f64)]),
    )
}

#[test]
fn empty_query_matches_anything() {
    let (s, n) = props();
    assert!(query_matches("", &s, &n));
}

#[test]
fn equality_and_comparisons() {
    let (s, n) = props();
    assert!(query_matches("+properties.mode:deathmatch", &s, &n));
    assert!(!query_matches("+properties.mode:capture", &s, &n));
    assert!(query_matches("+properties.rank:>=40", &s, &n));
    assert!(!query_matches("+properties.rank:>42", &s, &n));
    assert!(query_matches("-properties.mode:capture", &s, &n));
    assert!(!query_matches("-properties.mode:deathmatch", &s, &n));
}
