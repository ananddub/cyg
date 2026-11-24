//! A ratchet on the OpenAPI document.
//!
//! `auto_route` infers the response schema from the handler's return type, so a
//! handler that says `-> Response` documents nothing. This test compares the set
//! of undocumented routes with a checked-in baseline: a new handler that returns
//! an untyped reply fails here, and a handler that becomes typed fails here too
//! until its line is removed from the baseline. The file is the visible debt, not
//! a place for it to hide.
//!
//! Regenerate after converting handlers:
//! ```text
//! CYG_DUMP_OPENAPI=1 cargo test -p engine-api --lib openapi::undocumentation
//! ```

use std::collections::BTreeSet;
use std::path::Path;

const BASELINE: &str = "tests/openapi_undocumented.txt";

/// Engine probes, the console shell, and the static client assets. These are
/// not part of the game's DTO contract; the API routes all are.
fn is_shell(path: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "/metrics",
        "/readyz",
        "/status",
        "/auto",
        "/ws",
        "/dashboard",
        "/api/dashboard",
        "/health",
        "/openapi.json",
        "/Build/",
        "/TemplateData/",
        "/arena_map.json",
        "/cyg.js",
        "/cyg_core_rs.wasm",
        "/lobby/rooms",
        "/node_modules/",
        "/openoxide-client.js",
        "/packages/",
        "/unity",
    ];
    path == "/" || PREFIXES.iter().any(|prefix| path.starts_with(prefix))
}

fn undocumented() -> BTreeSet<String> {
    let mut rows = BTreeSet::new();
    for route in auto_route::documented_routes() {
        // The engine's own probes and the console's shell are not part of the
        // game API contract.
        if is_shell(route.path()) {
            continue;
        }
        let mut missing = Vec::new();
        if !route.documents_response() {
            missing.push("no response");
        }
        // A bodyless method has no request to describe; a path parameter is
        // already in the path template. A custom POST with no extractor is
        // bodyless too: it names its resource in the path, not in a payload.
        let bodyless = matches!(
            route.method().to_ascii_lowercase().as_str(),
            "get" | "delete" | "head" | "options"
        ) || !route.documents_request();
        if !route.documents_request() && !bodyless {
            missing.push("no request");
        }
        if !missing.is_empty() {
            rows.insert(format!("{}  ({})", route.operation(), missing.join(", ")));
        }
    }
    rows
}

fn baseline() -> BTreeSet<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(BASELINE);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("read {BASELINE}: {err}"))
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

#[test]
fn responses_name_a_concrete_dto() {
    let free_form: BTreeSet<String> = auto_route::documented_routes()
        .into_iter()
        .filter(|route| !is_shell(route.path()))
        .filter(|route| route.response_is_free_form())
        .map(|route| route.operation())
        .collect();
    assert!(
        free_form.is_empty(),
        "these routes still publish a free-form response; name the DTO with `response_body = <T>`: {free_form:#?}"
    );
}

#[test]
fn requests_are_not_free_form() {
    let free_form: BTreeSet<String> = auto_route::documented_routes()
        .into_iter()
        .filter(|route| !is_shell(route.path()))
        .filter(|route| route.request_is_free_form())
        .map(|route| route.operation())
        .collect();
    assert!(
        free_form.is_empty(),
        "these routes still publish a free-form request; name the DTO with `request_body = <T>`: {free_form:#?}"
    );
}

#[test]
fn undocumentation_does_not_grow() {
    let found = undocumented();
    if std::env::var_os("CYG_DUMP_OPENAPI").is_some() {
        let mut text = String::from(
            "# Routes whose OpenAPI entry has no schema yet. One line per gap:\n\
             # `<METHOD> <path>  (no response[, no request])`\n\
             # Regenerate: CYG_DUMP_OPENAPI=1 cargo test -p engine-api --lib openapi::undocumentation\n",
        );
        for row in &found {
            text.push_str(row);
            text.push('\n');
        }
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(BASELINE);
        std::fs::write(&path, text).expect("write the baseline");
        return;
    }

    let known = baseline();
    let new: Vec<_> = found.difference(&known).collect();
    let stale: Vec<_> = known.difference(&found).collect();
    assert!(
        new.is_empty(),
        "these routes lost their schema: {new:#?}\n\
         Return a typed `Result<Json<T>, V2Error>` instead of `Response`."
    );
    assert!(
        stale.is_empty(),
        "these routes now have a schema; drop them from {BASELINE}: {stale:#?}"
    );
}
