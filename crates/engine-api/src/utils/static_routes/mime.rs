pub(crate) const CONTENT_TYPES: &[(&str, &str)] = &[
    ("html", "text/html; charset=utf-8"),
    ("htm", "text/html; charset=utf-8"),
    ("js", "text/javascript; charset=utf-8"),
    ("mjs", "text/javascript; charset=utf-8"),
    ("cjs", "text/javascript; charset=utf-8"),
    ("css", "text/css; charset=utf-8"),
    ("json", "application/json"),
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

pub fn content_type_for(path: &str) -> &'static str {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
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
