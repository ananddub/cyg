use super::*;

/// The served spec must describe the whole registered v2 surface: for every
/// pinned route, a spec path whose static segments match and whose parameter
/// segments stand in for the fixture ids the pin uses, carrying the method.
#[test]
pub fn spec_covers_pinned_routes() {
    let spec = engine_api::auto_route::openapi_json();
    let paths = spec["paths"].as_object().expect("spec has a paths map");
    assert!(!paths.is_empty(), "the spec describes some routes");

    let mut missing = Vec::new();
    for (method, pinned) in pinned_routes() {
        if !pinned.starts_with("/v2/") {
            continue;
        }
        let pin_segments: Vec<&str> = pinned.trim_matches('/').split('/').collect();
        let covered = paths.iter().any(|(candidate, operations)| {
            let segments: Vec<&str> = candidate.trim_matches('/').split('/').collect();
            if segments.len() != pin_segments.len() {
                return false;
            }
            let shape = segments
                .iter()
                .zip(&pin_segments)
                .all(|(spec, pin)| spec.starts_with('{') || spec == pin);
            shape && operations.get(&method.to_lowercase()).is_some()
        });
        if !covered {
            missing.push(format!("{method} {pinned}"));
        }
    }
    assert!(
        missing.is_empty(),
        "the served spec is missing {} pinned route(s): {}",
        missing.len(),
        missing.join(", ")
    );
}
