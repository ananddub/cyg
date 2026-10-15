//! Static assets for the control plane.
//!
//! Replaces the in-transport `on_http` hook: the website page, the SDK bundle,
//! the arena map, the Unity WebGL build and the OpenOxide SDK files are served
//! from here, on the control port, with an mtime-keyed cache so a reload during
//! development picks up a new `openoxide.js` without a restart.
//!
//! Every path that reaches the filesystem goes through
//! [`StaticServer::resolve_under`], which rejects anything that is not a plain
//! child of the root it was given — no `..`, no absolute path, no escape via
//! percent-decoding. Two roots exist: the project directory (pages and their
//! assets) and the Unity WebGL build directory (a fixed tree the page
//! references with root-absolute `/Build/…` and `/TemplateData/…` URLs).

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::{OriginalUri, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;

use crate::ControlState;

/// A file plus the stamp it was read at. Re-reading only happens when the stat
/// disagrees, so a hot page is one `stat` per request, not one read.
///
/// The mtime is optional because not every filesystem reports one; the length
/// is not, and it is what catches a same-second rewrite on a coarse-granularity
/// filesystem.
#[derive(Clone)]
struct Cached {
    stamp: (Option<std::time::SystemTime>, u64),
    body: Arc<Vec<u8>>,
}

pub struct StaticServer {
    /// Project root — the only place a client-named page or asset may live.
    root: PathBuf,
    website: PathBuf,
    sdk_js: PathBuf,
    schema_mjs: PathBuf,
    /// Unity's exported build folder: `index.html` plus its own `Build/` and
    /// `TemplateData/` subtrees.
    unity_build: PathBuf,
    /// Bytes baked into the binary, used when the project has no local copy, so
    /// a fresh clone serves a working page instead of a 404.
    defaults: HashMap<String, Arc<Vec<u8>>>,
    cache: Mutex<HashMap<PathBuf, Cached>>,
}

impl StaticServer {
    /// `project_dir` is the directory holding `config.toml`; asset paths in the
    /// config are relative to it, exactly as `src/server.rs` treats them.
    pub fn new(project_dir: &Path, game: &engine_config::GameConfig) -> StaticServer {
        let defaults = HashMap::new();
        StaticServer {
            root: project_dir.to_path_buf(),
            website: project_dir.join(&game.website),
            sdk_js: game.sdk_dir.join("packages/sdk/dist/colyseus.js"),
            schema_mjs: game
                .sdk_dir
                .join("node_modules/@colyseus/schema/build/index.mjs"),
            unity_build: project_dir
                .parent()
                .map(|p| p.join("OpenOxideArena/Build/WebGL"))
                .unwrap_or_else(|| project_dir.join("unity")),
            defaults,
            cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn default_for(&self, path: &str) -> Option<Arc<Vec<u8>>> {
        self.defaults.get(path).cloned()
    }

    pub fn website_path(&self) -> &Path {
        &self.website
    }
    pub fn sdk_js_path(&self) -> &Path {
        &self.sdk_js
    }
    pub fn schema_mjs_path(&self) -> &Path {
        &self.schema_mjs
    }
    pub fn unity_build_path(&self) -> &Path {
        &self.unity_build
    }
    pub fn project_root(&self) -> &Path {
        &self.root
    }

    /// Read `path` if it exists, refreshing the cache only when its stamp
    /// changed. `None` on any stat or read failure — a missing asset is a 404,
    /// never a panic and never an empty body cached forever.
    pub fn read(&self, path: &Path) -> Option<Arc<Vec<u8>>> {
        // The only syscall on the hit path.
        let meta = std::fs::metadata(path).ok()?;
        if !meta.is_file() {
            return None;
        }
        let stamp = (meta.modified().ok(), meta.len());
        // Poison-tolerant: a panic somewhere else must not turn every later
        // asset request into a 404 for the life of the process. The map holds
        // only `Arc<[u8]>` and a stamp, so its invariants cannot be broken.
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(hit) = cache.get(path)
            && hit.stamp == stamp
        {
            return Some(hit.body.clone());
        }
        // Re-read happens on a changed stamp only; on failure nothing is
        // inserted, so the next request stats again instead of serving stale.
        let body = Arc::new(std::fs::read(path).ok()?);
        cache.insert(path.to_path_buf(), Cached { stamp, body: body.clone() });
        Some(body)
    }

    /// Map a URL path to a file under the project root, or `None` if the client
    /// asked for something outside it.
    pub fn resolve(&self, url_path: &str) -> Option<PathBuf> {
        self.resolve_under(&self.root, url_path)
    }

    /// Map a URL path to a plain child of `base`.
    ///
    /// Decoding happens here rather than in the extractor: `OriginalUri` hands
    /// us the raw path, and a request for `%2e%2e/etc` is only dangerous once
    /// it has been turned into `../etc`, so the escape check has to run on the
    /// decoded string. Decoding is single-pass — `%252e%252e` decodes to the
    /// literal name `%2e%2e`, never to `..`.
    pub fn resolve_under(&self, base: &Path, url_path: &str) -> Option<PathBuf> {
        let decoded = percent_decode(url_path)?;
        let mut out = base.to_path_buf();
        for seg in decoded.trim_matches('/').split('/') {
            match seg {
                "" | "." => continue,
                // Refused, never collapsed: a URL is not a filesystem path and
                // no client gets to name a file outside `base`.
                ".." => return None,
                _ => {
                    // A backslash is a separator on Windows and a literal here;
                    // an embedded NUL truncates the path for `open()`. Neither
                    // can be a child of `base`, so both are refused outright —
                    // including the `%5c` / `%00` forms the decoder just produced.
                    if seg.contains('\\') || seg.contains('\0') {
                        return None;
                    }
                    out.push(seg);
                }
            }
        }
        // Belt and braces: whatever the loop built must still sit inside `base`.
        out.strip_prefix(base).is_ok().then_some(out)
    }
}

/// Percent-decode a URI path without allocating when there is no escape, and
/// reject a result that is not valid UTF-8 — such a request cannot name a file
/// in this repository, and answering it would mean guessing at the client's
/// intent.
fn percent_decode(raw: &str) -> Option<Cow<'_, str>> {
    if !raw.contains('%') {
        return Some(Cow::Borrowed(raw));
    }
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(h), Some(l)) = (hex_nibble(bytes[i + 1]), hex_nibble(bytes[i + 2]))
        {
            out.push(h << 4 | l);
            i += 3;
            continue;
        }
        // A stray `%` is not an escape; keep it and let the lookup miss.
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).ok().map(Cow::Owned)
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// `pub fn router(&ControlState) -> Router<ControlState>` — the frozen entry
/// point `lib.rs` composes. The exact paths the legacy hook answered are
/// registered here; everything else is left to `fallback` below.
pub fn router(state: &ControlState) -> Router<ControlState> {
    let _ = state;
    Router::new()
        .route("/health", get(health))
        .route("/", get(serve_root))
        .route("/openoxide.js", get(serve_sdk))
        .route("/arena_map.json", get(serve_map))
        .route("/packages/sdk/dist/colyseus.js", get(serve_sdk_bundle))
        .route(
            "/node_modules/@colyseus/schema/build/index.mjs",
            get(serve_schema),
        )
        .route("/unity", get(serve_unity_index))
        .route("/unity/", get(serve_unity_index))
        .route("/unity/{*rest}", get(serve_unity_asset))
        // The exported `index.html` references its payload root-absolutely, so
        // the same tree is also reachable at the two prefixes Unity emits.
        .route("/Build/{*rest}", get(serve_unity_asset))
        .route("/TemplateData/{*rest}", get(serve_unity_asset))
}

/// Which Unity build-relative path a matched URL asks for, or `None` when the
/// URL is not one of the three Unity prefixes.
///
/// `/unity/…` drops its prefix and `/Build/…`, `/TemplateData/…` keep theirs:
/// the export already lays those two names out as directories under the build
/// folder, so the result joins onto `unity_build` unchanged.
fn unity_subpath(path: &str) -> Option<&str> {
    if let Some(sub) = path.strip_prefix("/unity/") {
        return Some(sub);
    }
    if path.starts_with("/Build/") || path.starts_with("/TemplateData/") {
        // Skip the leading slash only; both prefixes are already the directory
        // names Unity wrote into `index.html`.
        return Some(&path[1..]);
    }
    None
}

async fn health() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], "ok")
}

/// Anything unmatched is a project file request: `/tank-3d.html`, `/assets/…`,
/// `…/*.wasm`. Only paths that resolve under the project root are ever opened,
/// and a refusal is a plain 404 — telling the client "no" is enough; explaining
/// that it was a traversal attempt fingerprints the rule to anyone probing.
///
/// A fallback has no matched route, so the path comes from the request URI
/// rather than a `Path` extractor. `Uri::path()` is the raw, still-encoded path
/// and never carries the query string.
pub async fn fallback(
    State(state): State<ControlState>,
    OriginalUri(uri): OriginalUri,
) -> Response {
    let statics = &state.statics;
    let path = uri.path();
    match statics.resolve(path).and_then(|p| statics.read(&p)) {
        Some(body) => file_response(path, body),
        None => not_found(),
    }
}

/// Extension → MIME. Every value is a literal, so the response header is built
/// with `HeaderValue::from_static` — no allocation and no parse failure per
/// request, which is the whole reason this is a table instead of a registry
/// lookup.
///
/// It is broad on purpose: a Unity WebGL build and a three.js page ask for much
/// more than HTML. `*.wasm` has to be `application/wasm` or the browser gives up
/// streaming compilation for an `ArrayBuffer` fetch; the compressed
/// `*.data.unityweb` / `*.framework.js.unityweb` payloads must **not** be
/// advertised as text, since a charset in the header makes intermediaries
/// transcode the bytes and Unity's own decompressor then sees garbage.
const CONTENT_TYPES: &[(&str, &str)] = &[
    ("html", "text/html; charset=utf-8"),
    ("htm", "text/html; charset=utf-8"),
    ("js", "text/javascript; charset=utf-8"),
    ("mjs", "text/javascript; charset=utf-8"),
    ("cjs", "text/javascript; charset=utf-8"),
    ("css", "text/css; charset=utf-8"),
    ("json", "application/json"),
    // Source maps are JSON; devtools only parse them when the body is valid.
    ("map", "application/json"),
    ("wasm", "application/wasm"),
    // Unity's uncompressed payload pair.
    ("data", "application/octet-stream"),
    ("mem", "application/octet-stream"),
    ("unityweb", "application/octet-stream"),
    ("br", "application/octet-stream"),
    ("gz", "application/octet-stream"),
    ("bin", "application/octet-stream"),
    // glTF / three.js material assets.
    ("glb", "model/gltf-binary"),
    ("gltf", "model/gltf+json"),
    ("hdr", "image/vnd.radiance"),
    ("exr", "image/aces"),
    ("ktx", "image/ktx"),
    ("ktx2", "image/ktx2"),
    ("basis", "application/octet-stream"),
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
    ("avif", "image/avif"),
    ("bmp", "image/bmp"),
    ("tga", "image/x-tga"),
    // `favicon.ico` is what a Unity template asks for, hence the legacy alias.
    ("ico", "image/vnd.microsoft.icon"),
    ("svg", "image/svg+xml"),
    ("woff2", "font/woff2"),
    ("woff", "font/woff"),
    ("ttf", "font/ttf"),
    ("otf", "font/otf"),
    ("eot", "application/vnd.ms-fontobject"),
    ("txt", "text/plain; charset=utf-8"),
    ("xml", "application/xml"),
    ("mp3", "audio/mpeg"),
    ("ogg", "audio/ogg"),
    ("oga", "audio/ogg"),
    ("wav", "audio/wav"),
    ("mp4", "video/mp4"),
    ("webm", "video/webm"),
];

/// Content type for a URL path (or a filesystem path — only the last segment's
/// extension is read).
pub fn content_type_for(path: &str) -> &'static str {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    // The extension is the text after the final dot *of the file name*, so
    // `/v1.2/app` and `/app.` both mean "no extension".
    let ext = match name.rfind('.') {
        Some(i) => &name[i + 1..],
        None => "",
    };
    CONTENT_TYPES
        .iter()
        .find(|(want, _)| ext.eq_ignore_ascii_case(want))
        .map(|(_, mime)| *mime)
        .unwrap_or("application/octet-stream")
}

fn file_response(path: &str, body: Arc<Vec<u8>>) -> Response {
    // `no-cache` rather than `immutable`: a page under development reloads
    // against a rebuilt bundle, and a stale SDK copy is the classic "why is it
    // still broken". The server side is already re-stamped per request, so the
    // browser revalidating costs a stat, not a read.
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static(content_type_for(path)),
    );
    headers.insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-cache, must-revalidate"),
    );
    (headers, body.to_vec()).into_response()
}

fn not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        [(header::CONTENT_TYPE, header::HeaderValue::from_static("text/plain; charset=utf-8"))],
        "404 not found",
    )
        .into_response()
}

/// `/openoxide.js`: the project's own copy when it has one, so a game can
/// override the client glue, else the baked-in example.
async fn serve_sdk(State(state): State<ControlState>) -> Response {
    let s = &state.statics;
    let path = s.project_root().join("openoxide.js");
    match s.read(&path).or_else(|| s.default_for("/openoxide.js")) {
        Some(body) => file_response("/openoxide.js", body),
        None => not_found(),
    }
}

/// `/`: redirect to the control dashboard. Engine does not render/inject game websites.
async fn serve_root() -> Response {
    axum::response::Redirect::temporary("/dashboard/").into_response()
}

async fn serve_map(State(state): State<ControlState>) -> Response {
    let s = &state.statics;
    let path = s.project_root().join("arena_map.json");
    match s.read(&path) {
        Some(body) => file_response("arena_map.json", body),
        None => not_found(),
    }
}

async fn serve_sdk_bundle(State(state): State<ControlState>) -> Response {
    let s = &state.statics;
    match s.read(s.sdk_js_path()) {
        Some(body) => file_response("sdk.js", body),
        None => not_found(),
    }
}

async fn serve_schema(State(state): State<ControlState>) -> Response {
    let s = &state.statics;
    match s.read(s.schema_mjs_path()) {
        Some(body) => file_response("index.mjs", body),
        None => not_found(),
    }
}

/// `/unity` and `/unity/`: the build's own landing page.
async fn serve_unity_index(State(state): State<ControlState>) -> Response {
    let s = &state.statics;
    match s.read(&s.unity_build_path().join("index.html")) {
        Some(body) => file_response("index.html", body),
        None => not_found(),
    }
}

/// `/unity/{*rest}`, `/Build/{*rest}` and `/TemplateData/{*rest}`, one handler
/// for the three because they name the same tree — see [`unity_subpath`].
///
/// The build root gets its own containment check, so `/Build/../../etc/passwd`
/// reaches neither the project nor the filesystem, and a file the export does
/// not contain is a 404 rather than a directory listing or a panic.
async fn serve_unity_asset(
    State(state): State<ControlState>,
    OriginalUri(uri): OriginalUri,
) -> Response {
    let s = &state.statics;
    let Some(sub) = unity_subpath(uri.path()) else {
        return not_found();
    };
    match s.resolve_under(s.unity_build_path(), sub).and_then(|p| s.read(&p)) {
        Some(body) => file_response(sub, body),
        None => not_found(),
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use super::*;

    fn server(root: &Path) -> StaticServer {
        StaticServer::new(root, &engine_config::GameConfig::default())
    }

    /// A throwaway directory under the system temp dir, removed again by `Drop`
    /// so a test that asserts cannot leave litter for the next run.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> TempDir {
            let dir = std::env::temp_dir().join(format!(
                "openoxide-static-{tag}-{}-{}",
                std::process::id(),
                static_counter()
            ));
            std::fs::create_dir_all(&dir).expect("create temp dir");
            TempDir(dir)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn static_counter() -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        N.fetch_add(1, Ordering::Relaxed)
    }

    #[test]
    fn resolve_accepts_plain_children() {
        let dir = TempDir::new("resolve");
        let s = server(dir.path());
        assert_eq!(s.resolve("/tank-3d.html"), Some(dir.path().join("tank-3d.html")));
        assert_eq!(
            s.resolve("/assets/player.png"),
            Some(dir.path().join("assets").join("player.png"))
        );
        // A duplicated or dot segment stays inside the root.
        assert_eq!(s.resolve("//assets//a.wasm"), s.resolve("/assets/a.wasm"));
        assert_eq!(s.resolve("/./assets/a.json"), s.resolve("/assets/a.json"));
    }

    #[test]
    fn resolve_rejects_every_way_out_of_the_root() {
        let dir = TempDir::new("escape");
        let s = server(dir.path());
        for probe in [
            "/../etc/passwd",
            "/a/../../etc/passwd",
            "/%2e%2e/etc/passwd",
            "/.%2e/.%2e/%2e%2e/etc/passwd",
            r"/..\..\etc\passwd",
            r"/a/..\b",
            "/%5c%5cetc/passwd",
            "/a%00b",
            "/a\0b",
        ] {
            assert_eq!(s.resolve(probe), None, "{probe:?} escaped the root");
        }
        // Decoding is single-pass, so `%252e%252e` becomes the literal name
        // `%2e%2e` rather than a parent — still a child of the root, and still
        // not a file anybody shipped.
        let once = s.resolve("/%252e%252e/x").expect("a decoded name is not a traversal");
        assert!(once.starts_with(dir.path()));
    }

    #[test]
    fn content_type_covers_the_unity_and_asset_extensions() {
        assert_eq!(content_type_for("/Build/WebGL.wasm"), "application/wasm");
        assert_eq!(content_type_for("/Build/WebGL.data"), "application/octet-stream");
        assert_eq!(
            content_type_for("/Build/WebGL.framework.js.unityweb"),
            "application/octet-stream"
        );
        assert_eq!(
            content_type_for("/Build/WebGL.data.unityweb"),
            "application/octet-stream"
        );
        assert_eq!(content_type_for("/Build/WebGL.loader.js"), "text/javascript; charset=utf-8");
        assert_eq!(content_type_for("/TemplateData/style.css"), "text/css; charset=utf-8");
        assert_eq!(content_type_for("/TemplateData/favicon.ico"), "image/vnd.microsoft.icon");
        assert_eq!(content_type_for("/arena_map.json"), "application/json");
        assert_eq!(content_type_for("/openoxide.js.map"), "application/json");
        assert_eq!(content_type_for("/fonts/ui.woff2"), "font/woff2");
        assert_eq!(content_type_for("/models/tank.glb"), "model/gltf-binary");
        assert_eq!(content_type_for("/sky/noon.hdr"), "image/vnd.radiance");
        // Case and extension-less names do not upset it.
        assert_eq!(content_type_for("/BUILD/WEBGL.WASM"), "application/wasm");
        assert_eq!(content_type_for("/LICENSE"), "application/octet-stream");
        assert_eq!(content_type_for("/v1.2/app"), "application/octet-stream");
        // Every declared type is a header value hyper accepts without parsing.
        for (_, mime) in CONTENT_TYPES {
            assert!(header::HeaderValue::from_str(mime).is_ok(), "{mime}");
        }
    }

    #[test]
    fn game_web_is_not_embedded_in_server() {
        let dir = TempDir::new("defaults");
        let s = server(dir.path());
        assert!(s.default_for("/website.html").is_none());
        assert!(s.default_for("/openoxide.js").is_none());
        assert!(s.default_for("/no-such-default").is_none());
    }

    #[test]
    fn reads_a_file_then_notices_a_rewrite() {
        let dir = TempDir::new("read");
        let s = server(dir.path());
        let file = dir.path().join("page.html");

        assert_eq!(s.read(&file), None, "a missing file is not a cached empty body");

        std::fs::write(&file, b"<html>first</html>").unwrap();
        assert_eq!(&*s.read(&file).unwrap(), b"<html>first</html>");
        // Second read is the cache hit: same Arc, no re-read.
        let hit = s.read(&file).unwrap();
        assert!(Arc::ptr_eq(&hit, &s.read(&file).unwrap()));

        // Same mtime second is exactly the case a 1s-granularity filesystem
        // hides, so force the stamp to move before comparing.
        let next = SystemTime::now() + Duration::from_secs(2);
        std::fs::write(&file, b"<html>second, longer</html>").unwrap();
        filetime(&file, next);
        assert_eq!(&*s.read(&file).unwrap(), b"<html>second, longer</html>");

        // Third write, with its mtime pinned back to what the cache already
        // holds: only the length disagrees, which is the case a rebuild inside
        // one clock tick hides.
        std::fs::write(&file, b"<html>third, same!</html>").unwrap();
        filetime(&file, next);
        assert_eq!(&*s.read(&file).unwrap(), b"<html>third, same!</html>");

        std::fs::remove_file(&file).unwrap();
        assert_eq!(s.read(&file), None);
    }

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
        assert_eq!(
            unity_subpath("/TemplateData/style.css"),
            Some("TemplateData/style.css")
        );
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
        // stub handler stands in for the `ControlState`-bound real ones.
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
        let _ = router;
    }

    async fn stub_page() -> &'static str {
        "stub"
    }

    /// Set both times so a rewrite lands on a stamp the cache has not seen;
    /// `atime` is irrelevant to the cache, `mtime` is the whole check.
    fn filetime(path: &Path, when: SystemTime) {
        let ft = std::fs::FileTimes::new().set_accessed(when).set_modified(when);
        std::fs::OpenOptions::new()
            .write(true)
            .open(path)
            .and_then(|f| f.set_times(ft))
            .expect("set file times");
    }
}
