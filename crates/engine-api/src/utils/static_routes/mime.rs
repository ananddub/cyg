/// Extension → MIME. Every value is a literal, so the header is built with
/// `HeaderValue::from_static` — no allocation or parse failure per request.
/// Broad on purpose: `*.wasm` must be `application/wasm` (or streaming compilation
/// is lost) and compressed `*.unityweb` must not carry a charset, or intermediaries
/// transcode the bytes.
pub(crate) const CONTENT_TYPES: &[(&str, &str)] = &[
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
    ("data", "application/octet-stream"),
    ("mem", "application/octet-stream"),
    ("unityweb", "application/octet-stream"),
    ("br", "application/octet-stream"),
    ("gz", "application/octet-stream"),
    ("bin", "application/octet-stream"),
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
