use super::{flag_value, has_flag, usage};

/// `cyg-game openapi [--out spec.json] [--check golden.json]` — emit the
/// served OpenAPI document. `--check` compares the freshly generated spec
/// against a committed golden file (canonical JSON), so an unintended API
/// shape change fails before it ships.
pub fn run(args: &[String]) {
    if has_flag(args, "--help") || has_flag(args, "-h") {
        usage("openapi [--out spec.json] [--check golden.json]");
    }
    let spec = engine_api::auto_route::openapi_json();
    if let Some(golden) = flag_value(args, "--check") {
        let stored = match std::fs::read_to_string(&golden) {
            Ok(stored) => stored,
            Err(e) => {
                tracing::error!("✗ openapi: cannot read {golden}: {e}");
                std::process::exit(1);
            }
        };
        let stored: serde_json::Value = match serde_json::from_str(&stored) {
            Ok(value) => value,
            Err(e) => {
                tracing::error!("✗ openapi: {golden} is not valid JSON: {e}");
                std::process::exit(1);
            }
        };
        let fresh = canonical(&spec);
        let stored = canonical(&stored);
        if fresh == stored {
            tracing::info!("✓ the served spec matches {golden}");
            return;
        }
        tracing::error!("✗ openapi: the served spec drifted from {golden}");
        report_drift(&stored, &fresh);
        std::process::exit(1);
    }
    let rendered =
        serde_json::to_string_pretty(&spec).expect("the spec is valid JSON by construction");
    match flag_value(args, "--out") {
        Some(out) => {
            if let Some(parent) = std::path::Path::new(&out).parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            match std::fs::write(&out, rendered + "\n") {
                Ok(()) => tracing::info!("✓ spec written to {out}"),
                Err(e) => {
                    tracing::error!("✗ openapi: cannot write {out}: {e}");
                    std::process::exit(1);
                }
            }
        }
        None => println!("{rendered}"),
    }
}

fn canonical(value: &serde_json::Value) -> String {
    value.to_string()
}

fn report_drift(stored: &str, fresh: &str) {
    let stored_paths = paths(stored);
    let fresh_paths = paths(fresh);
    for path in stored_paths.difference(&fresh_paths) {
        tracing::error!("  removed: {path}");
    }
    for path in fresh_paths.difference(&stored_paths) {
        tracing::error!("  added:   {path}");
    }
}

/// The `<method> <path>` pairs of a rendered spec string, extracted from the
/// serialised JSON so drift is reported in route terms, not JSON terms.
fn paths(spec: &str) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(spec) {
        if let Some(items) = value.get("paths").and_then(|p| p.as_object()) {
            for (path, operations) in items {
                if let Some(operations) = operations.as_object() {
                    for method in operations.keys() {
                        out.insert(format!("{method} {path}"));
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn drift_is_reported_in_route_terms() {
        let stored = json!({"paths": {"/v2/account": {"get": {}}, "/v2/gone": {"post": {}}}});
        let fresh = json!({"paths": {"/v2/account": {"get": {}}, "/v2/new": {"post": {}}}});
        let mut out = String::new();
        // the report goes to tracing; assert on the sets directly
        let stored = paths(&stored.to_string());
        let fresh = paths(&fresh.to_string());
        assert_eq!(stored.difference(&fresh).count(), 1, "/v2/gone removed");
        assert_eq!(fresh.difference(&stored).count(), 1, "/v2/new added");
        let _ = &mut out;
    }

    #[test]
    fn canonical_form_is_key_order_insensitive() {
        let a = json!({"paths": {"a": {}}, "info": {}});
        let b = json!({"info": {}, "paths": {"a": {}}});
        assert_eq!(canonical(&a), canonical(&b));
    }
}
