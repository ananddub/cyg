use super::{TempDir, server};
use crate::static_routes::content_type_for;
use crate::static_routes::routes::unity_subpath;

#[test]
fn the_fallback_shape_serves_only_children_of_the_root() {
    let dir = TempDir::new("fallback");
    let s = server(dir.path());
    std::fs::write(dir.path().join("tank-3d.html"), b"<html>arena</html>").unwrap();

    let rel = s.resolve("/tank-3d.html").unwrap();
    let body = s.read(&rel).expect("project file is served");
    assert_eq!(&*body, b"<html>arena</html>");
    assert_eq!(content_type_for("/tank-3d.html"), "text/html; charset=utf-8");

    // A directory is not a file, and a name we never wrote is a 404.
    assert_eq!(s.read(dir.path()), None);
    assert_eq!(s.resolve("/missing.html").and_then(|p| s.read(&p)), None);
}

#[test]
fn unity_urls_land_inside_the_build_directory() {
    assert_eq!(unity_subpath("/unity/index.html"), Some("index.html"));
    // The two prefixes Unity writes into its own `index.html` keep the
    // directory name, because that name is part of the exported tree.
    assert_eq!(unity_subpath("/Build/WebGL.wasm"), Some("Build/WebGL.wasm"));
    assert_eq!(unity_subpath("/TemplateData/style.css"), Some("TemplateData/style.css"));
    assert_eq!(unity_subpath("/unity"), None);
    assert_eq!(unity_subpath("/Build"), None);
    assert_eq!(unity_subpath("/tank-3d.html"), None);

    // And the build root is contained in its own right, not the project's.
    let dir = TempDir::new("unity");
    let s = server(dir.path());
    let build = s.unity_build_path();
    assert_eq!(
        s.resolve_under(build, "Build/WebGL.wasm"),
        Some(build.join("Build").join("WebGL.wasm"))
    );
    assert_eq!(s.resolve_under(build, "../../etc/passwd"), None);
    assert_eq!(s.resolve_under(build, "%2e%2e/%2e%2e/etc/passwd"), None);
}

#[test]
fn the_route_table_registers_without_conflicting() {
    // Axum validates the path tree when a route is added and panics on an
    // ambiguous pattern, so building the table is itself the assertion. The
    // stub handler stands in for the `ApiState`-bound real ones.
    use axum::Router;
    use axum::routing::get;
    let _router = Router::<()>::new()
        .route("/", get(stub_page))
        .route("/website.html", get(stub_page))
        .route("/openoxide.js", get(stub_page))
        .route("/arena_map.json", get(stub_page))
        .route("/health", get(stub_page))
        .route("/packages/sdk/dist/colyseus.js", get(stub_page))
        .route("/node_modules/@colyseus/schema/build/index.mjs", get(stub_page))
        .route("/unity", get(stub_page))
        .route("/unity/", get(stub_page))
        .route("/unity/{*rest}", get(stub_page))
        .route("/Build/{*rest}", get(stub_page))
        .route("/TemplateData/{*rest}", get(stub_page));
    let _ = crate::static_routes::router;
}

async fn stub_page() -> &'static str {
    "stub"
}
