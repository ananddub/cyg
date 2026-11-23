//! The layering rules the crate claims, checked against the source.
//!
//! These are invariants, not style: a query in a service means the service layer
//! can no longer be tested without a store, and a `current_state()` call in a
//! service means its dependencies are invisible.

use std::fs;
use std::path::{Path, PathBuf};

fn sources(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect(dir, &mut out);
    out
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("source directory is readable") {
        let path = entry.expect("directory entry is readable").path();
        if path.is_dir() {
            collect(&path, out);
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).expect("source file is readable")
}

fn offenders(files: &[PathBuf], predicate: impl Fn(&str) -> bool) -> Vec<String> {
    files
        .iter()
        .filter(|path| predicate(&read(path)))
        .map(|path| path.display().to_string())
        .collect()
}

#[test]
fn services_hold_no_query_text() {
    // Query strings are the repository layer's business. A service that pastes
    // one is a service that cannot be reasoned about without a database.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/services");
    let files = sources(&root);
    let found = offenders(&files, |text| {
        [
            "\"SELECT ",
            "\"UPDATE ",
            "\"CREATE ",
            "\"DELETE FROM ",
            "\"UPSERT ",
        ]
        .iter()
        .any(|needle| text.contains(needle))
    });
    assert!(
        found.is_empty(),
        "these services carry query text; move it into repositories/: {found:#?}"
    );
}

#[test]
fn services_do_not_reach_for_the_ambient_state() {
    // `current_state()` is a task-local installed by the HTTP layer. Reading it
    // from a service hides a dependency and makes the service unusable outside a
    // request. Controllers and middleware may use it.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/services");
    let files = sources(&root);
    // `realtime::presence::global` is the one hold-out: it is a process-wide
    // singleton built lazily the first time a socket joins, and the socket layer
    // has no container to resolve from. It is fine only while `Plane::for_tests`
    // keeps it constructible; everything else injects.
    let allowed = ["realtime/presence/realtime_presence_service.rs"];
    let found: Vec<String> = offenders(&files, |text| text.contains("current_state"))
        .into_iter()
        .filter(|path| {
            !allowed
                .iter()
                .any(|allowed| path.ends_with(&format!("src/services/{allowed}")))
        })
        .collect();
    assert!(
        found.is_empty(),
        "these services read the ambient request state: {found:#?}"
    );
}

#[test]
fn the_service_locator_stays_where_it_was_left() {
    // `deps::q` first asks the request's DI container and only then the state bag,
    // so it is the sanctioned accessor while services still take `&ApiState`.
    // This pins the set of domains that use it: a new one is a decision to review,
    // not a line to slip in.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/services");
    let mut domains: Vec<String> = sources(&root)
        .iter()
        .filter(|path| {
            let text = read(path);
            text.contains("deps::q(") || text.contains("deps::blobs(")
        })
        .filter_map(|path| {
            path.strip_prefix(&root)
                .ok()?
                .components()
                .next()
                .map(|first| first.as_os_str().to_string_lossy().into_owned())
        })
        .collect();
    domains.sort();
    domains.dedup();
    assert_eq!(
        domains,
        vec!["realtime"],
        "the service locator moved; update this list with the reason"
    );
}

/// The number of service methods that still take `&ApiState`.
///
/// Each one is a method whose dependencies are invisible: it can only be called
/// by something that already has the whole plane. The fix is constructor
/// injection — a service holds the handles it needs via `Arc<Plane>` and
/// `#[singleton] fn new` resolves them.
///
/// This is a one-way ratchet. Converting a method lowers the baseline in
/// `tests/api_state_param_baseline.txt`; nothing may raise it. The remaining two
/// are deliberate: `notify::outbox` builds an unsubscribe link from `API_BASE`
/// and takes the state whole to do it, and one free helper has no owner to
/// inject into. Both are queued for deletion, not for growth.
#[test]
fn the_state_parameter_only_shrinks() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/services");
    let files = sources(&root);
    let count: usize = files
        .iter()
        .map(|path| read(path).matches("state: &ApiState").count())
        .sum();

    let baseline: usize = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/api_state_param_baseline.txt"),
    )
    .expect("the baseline file is readable")
    .trim()
    .parse()
    .expect("the baseline is a number");

    assert!(
        count <= baseline,
        "a service method grew a `state: &ApiState` parameter ({count} > {baseline}); \
         inject what it needs instead"
    );
    assert!(
        count == baseline,
        "services lost {baseline_minus} `state: &ApiState` parameter(s) — lower the baseline to {count}",
        baseline_minus = baseline - count
    );
}
