//! Serves the built dashboard: `/dashboard` and its `/dashboard/assets/*`.
//!
//! Owned by the dashboard workstream. `lib.rs` merges [`router`]; the plan is
//! `npm run build` → `dashboard/dist` → this module, so one binary answers the
//! game website *and* the operator dashboard.
//!
//! Nothing here compiles or bundles: if `dashboard/dist` is absent the routes
//! answer a plain "not built yet" page, because a 404 from an unvisited route is
//! indistinguishable from a server that is down.
//!
//! The dist directory is resolved per request rather than at startup, and files
//! are read per request rather than cached. A dashboard build is a 300 KB asset
//! behind an operator's refresh, and the alternative — pinning the tree at boot —
//! means every rebuild needs a server restart.

use std::path::{Path, PathBuf};

use axum::Router;
use axum::extract::OriginalUri;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;

use crate::ControlState;

/// Overrides the walk-up search entirely: the directory Vite wrote `index.html`
/// into (`dashboard/dist`).
const ENV_DIST: &str = "OX_DASHBOARD_DIST";

/// Where a `cargo run` from the repo root finds the build, relative to any
/// ancestor directory.
const DIST_SUBPATH: &str = "dashboard/dist";

/// URL prefix whose remainder names a file under the dist root.
const ASSET_PREFIX: &str = "/dashboard/assets/";

/// `pub fn router(&ControlState) -> Router<ControlState>` — frozen entry point.
pub fn router(state: &ControlState) -> Router<ControlState> {
    let _ = state;
    Router::new()
        .route("/dashboard", get(index))
        .route("/dashboard/", get(index))
        .route("/dashboard/assets/{*rest}", get(asset))
}

async fn index() -> Response {
    serve_index(dist_root().as_deref())
}

/// The wildcard segment is taken from [`OriginalUri`], not a `Path` extractor,
/// so this handler sees exactly the bytes the client sent — including a raw or
/// encoded `..` — instead of a value some other layer already normalised.
async fn asset(OriginalUri(uri): OriginalUri) -> Response {
    let Some(rest) = uri.path().strip_prefix(ASSET_PREFIX) else {
        return not_found();
    };
    serve_asset(dist_root().as_deref(), rest)
}

/// The dist root for this request, or `None` when the dashboard is not built.
///
/// `$OX_DASHBOARD_DIST` wins so a packager or an out-of-tree build can point the
/// server at a copy that is not next to the source; otherwise walk up from the
/// process cwd, which is the repo root under `cargo run`. Every candidate has to
/// actually hold an `index.html`, so a stale env var falls back to the search
/// instead of serving nothing.
fn dist_root() -> Option<PathBuf> {
    if let Some(from_env) = std::env::var_os(ENV_DIST)
        && let Some(dir) = usable_dist(Path::new(&from_env))
    {
        return Some(dir);
    }
    let mut dir = std::env::current_dir().ok()?;
    loop {
        if let Some(found) = usable_dist(&dir.join(DIST_SUBPATH)) {
            return Some(found);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Canonical path of `dir`, but only if `dir/index.html` is a readable file.
///
/// Canonicalising here means the root every asset is compared against has
/// symlinks and `..` already resolved, so the per-asset containment check cannot
/// be fooled by a temp directory reached through a link (`/tmp` on macOS, a
/// mounted build volume).
fn usable_dist(dir: &Path) -> Option<PathBuf> {
    if !dir.join("index.html").is_file() {
        return None;
    }
    std::fs::canonicalize(dir).ok()
}

/// `/dashboard` and `/dashboard/`: the app shell, always revalidated.
fn serve_index(root: Option<&Path>) -> Response {
    let Some(dist) = root else {
        return not_built(StatusCode::OK);
    };
    match std::fs::read(dist.join("index.html")) {
        Ok(body) => {
            // `no-store` because a rebuilt shell references a renamed bundle; a
            // cached copy of the old `index.html` is a blank screen pointing at
            // a hashed asset the new build no longer emits.
            html_response(body, "no-store")
        }
        // The root was usable a moment ago and its shell vanished (a rebuild in
        // progress, a deleted tree): the same page as an unbuilt dashboard.
        Err(_) => not_built(StatusCode::OK),
    }
}

/// `/dashboard/assets/{*rest}`, `rest` being the raw tail of the request URI.
fn serve_asset(root: Option<&Path>, rest: &str) -> Response {
    let Some(dist) = root else {
        // Same page, but a 404: a `200 text/plain` answer to a `.js` request is
        // how a browser ends up "parsing" HTML as a module.
        return not_built(StatusCode::NOT_FOUND);
    };
    let Some(path) = child_under(dist, rest) else {
        // A traversal probe and a typo get the same answer; explaining which one
        // it was tells an attacker which rule fired.
        return not_found();
    };
    match std::fs::read(&path) {
        Ok(body) => {
            let mime = crate::static_routes::content_type_for(&path.to_string_lossy());
            // Vite content-hashes every name under `assets/`, so a byte-identical
            // URL is a byte-identical file forever and a year of freshness costs
            // nothing. A rebuild changes the name, not the content of the URL.
            file_response(body, mime, "public, max-age=31536000, immutable")
        }
        Err(_) => not_found(),
    }
}

/// Map a URL tail to a plain child of `dist`, or `None` when it asks for
/// anything else.
///
/// Refusal comes before the filesystem: a segment containing `..`, a backslash
/// (a separator on Windows, where this binary may run), a NUL (which truncates
/// the path passed to `open`) or a `%` (a hashed bundle name never has one, so
/// it can only be an encoding trick to be reinterpreted downstream) never joins
/// onto the root at all. An absolute request tail is refused for the same reason
/// — a leading `/` would make `Path::join` discard the root entirely.
///
/// Canonicalising the candidate and comparing against the root is the backstop
/// for what the segment rules cannot see: a symlink the build happens to ship.
fn child_under(dist: &Path, rest: &str) -> Option<PathBuf> {
    if rest.is_empty() || rest.starts_with('/') {
        return None;
    }
    let mut out = dist.to_path_buf();
    for seg in rest.split('/') {
        match seg {
            "" | "." => continue,
            ".." => return None,
            _ => {
                if seg.contains(['\\', '\0', '%']) {
                    return None;
                }
                out.push(seg);
            }
        }
    }
    let found = std::fs::canonicalize(out).ok()?;
    found.is_file().then_some(found).filter(|p| p.starts_with(dist))
}

fn html_response(body: Vec<u8>, cache: &'static str) -> Response {
    file_response(body, "text/html; charset=utf-8", cache)
}

fn file_response(body: Vec<u8>, mime: &'static str, cache: &'static str) -> Response {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, header::HeaderValue::from_static(mime)),
            (header::CACHE_CONTROL, header::HeaderValue::from_static(cache)),
        ],
        body,
    )
        .into_response()
}

/// The honest answer when there is no build to serve: which fix to run, rather
/// than a bare status a cold operator cannot act on.
fn not_built(status: StatusCode) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, header::HeaderValue::from_static("text/plain; charset=utf-8"))],
        "dashboard not built yet — run `npm install && npm run build` in the dashboard/ directory, or set OX_DASHBOARD_DIST\n",
    )
        .into_response()
}

fn not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        [(header::CONTENT_TYPE, header::HeaderValue::from_static("text/plain; charset=utf-8"))],
        "404 not found",
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A throwaway dist tree under the system temp dir, deleted on `Drop` so the
    /// next run does not read a previous run's files.
    struct TempDist {
        dist: PathBuf,
    }

    impl TempDist {
        fn new(tag: &str) -> TempDist {
            let root = std::env::temp_dir().join(format!(
                "openoxide-dashboard-assets-{tag}-{}-{}",
                std::process::id(),
                counter()
            ));
            let dist = root.join(DIST_SUBPATH);
            std::fs::create_dir_all(dist.join("assets")).expect("create temp dist");
            std::fs::write(dist.join("index.html"), b"<!doctype html><title>OpenOxide</title>")
                .expect("write index");
            std::fs::write(dist.join("assets").join("app-9a7c.js"), b"console.log(1)")
                .expect("write asset");
            // Canonicalised: `dist_root` hands out canonical paths, and a
            // containment check against a non-canonical root would disagree with
            // production on any filesystem where the temp dir is a symlink.
            TempDist { dist: std::fs::canonicalize(dist).expect("canonicalize temp dist") }
        }

        fn dist(&self) -> &Path {
            &self.dist
        }
    }

    impl Drop for TempDist {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dist);
            let _ = std::fs::remove_dir_all(self.dist.parent().unwrap().parent().unwrap());
        }
    }

    fn counter() -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        N.fetch_add(1, Ordering::Relaxed)
    }

    fn header_of(response: &Response, name: header::HeaderName) -> Option<String> {
        response.headers().get(name).and_then(|v| v.to_str().ok()).map(str::to_string)
    }

    async fn parts(response: Response) -> (StatusCode, Option<String>, Option<String>, Vec<u8>) {
        let status = response.status();
        let content_type = header_of(&response, header::CONTENT_TYPE);
        let cache = header_of(&response, header::CACHE_CONTROL);
        let body = axum::body::to_bytes(response.into_body(), 1 << 20).await.expect("body");
        (status, content_type, cache, body.to_vec())
    }

    /// Every way a client can spell "go up", raw and encoded, must reach nothing.
    #[tokio::test]
    async fn no_spelling_of_a_parent_directory_escapes_the_dist_root() {
        let tree = TempDist::new("traversal");
        for probe in [
            "../../etc/passwd",
            "../../../etc/passwd",
            "..%2f..%2fetc%2fpasswd",
            "%2e%2e/%2e%2e/etc/passwd",
            "%2E%2E%2f%2E%2E%2fetc%2fpasswd",
            "..\\..\\etc\\passwd",
            "%5c%5c..%5cetc%5cpasswd",
            "/etc/passwd",
            "//etc/passwd",
            "assets/../../etc/passwd",
            "assets/%2e%2e/%2e%2e/%2e%2e/etc/passwd",
            "a%00pp.js",
            r"app.js\0",
            "%252e%252e/etc/passwd",
            "....//....//etc/passwd",
            "app.js/%25",
            "app.js/%",
        ] {
            assert_eq!(child_under(tree.dist(), probe), None, "{probe:?} was not refused");
            let (status, _, _, body) = parts(serve_asset(Some(tree.dist()), probe)).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{probe:?} served something");
            let text = String::from_utf8_lossy(&body);
            assert!(!text.contains("root:") && !text.contains("passwd:"), "{probe:?} leaked: {text}");
        }
    }

    #[tokio::test]
    async fn a_built_tree_serves_its_shell_and_a_hashed_asset() {
        let tree = TempDist::new("serve");

        let (status, mime, cache, body) = parts(serve_index(Some(tree.dist()))).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(mime.as_deref(), Some("text/html; charset=utf-8"));
        assert_eq!(cache.as_deref(), Some("no-store"), "a rebuilt shell must not be cached");
        assert!(body.starts_with(b"<!doctype html>"));

        let (status, mime, cache, body) =
            parts(serve_asset(Some(tree.dist()), "assets/app-9a7c.js")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(mime.as_deref(), Some("text/javascript; charset=utf-8"));
        assert_eq!(cache.as_deref(), Some("public, max-age=31536000, immutable"));
        assert_eq!(body, b"console.log(1)");

        // A name the build does not have is a 404, and so is the directory.
        let (status, _, _, _) = parts(serve_asset(Some(tree.dist()), "assets/missing.js")).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (status, _, _, _) = parts(serve_asset(Some(tree.dist()), "assets")).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "the assets directory is not a file");
    }

    #[tokio::test]
    async fn an_unbuilt_dashboard_answers_instead_of_404ing_silently() {
        let (status, mime, _, body) = parts(serve_index(None)).await;
        assert_eq!(status, StatusCode::OK, "an unbuilt dashboard is not an error page");
        assert_eq!(mime.as_deref(), Some("text/plain; charset=utf-8"));
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("not built"), "{text}");
        assert!(text.contains("npm run build"), "{text} must name the fix");

        // The same page for an asset, but not as a success: a browser that gets
        // `200` for a module request will try to run plain text as JavaScript.
        let resp = serve_asset(None, "assets/app-9a7c.js");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        assert_eq!(header_of(&resp, header::CONTENT_TYPE).as_deref(), Some("text/plain; charset=utf-8"));
    }

    #[test]
    fn a_dist_without_an_index_is_not_a_dist() {
        let empty = std::env::temp_dir().join(format!(
            "openoxide-dashboard-assets-empty-{}-{}",
            std::process::id(),
            counter()
        ));
        std::fs::create_dir_all(&empty).unwrap();
        assert_eq!(usable_dist(&empty), None, "no index.html, so no dashboard");
        std::fs::write(empty.join("index.html"), b"<html>x</html>").unwrap();
        let found = usable_dist(&empty).expect("a directory holding index.html is the dist");
        assert!(found.join("index.html").is_file());
        let _ = std::fs::remove_dir_all(&empty);
    }

    #[test]
    fn the_route_table_registers_without_conflicting() {
        // Axum validates the path tree when a route is added and panics on an
        // ambiguous pattern, so building the table is itself the assertion. These
        // handlers take no state, so a `Router<()>` can hold the real ones.
        let _router = Router::<()>::new()
            .route("/dashboard", get(index))
            .route("/dashboard/", get(index))
            .route("/dashboard/assets/{*rest}", get(asset));
    }
}
