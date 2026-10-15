use axum::http::header;

use crate::utils::static_routes::content_type_for;
use crate::utils::static_routes::mime::CONTENT_TYPES;

#[test]
fn content_type_covers_the_unity_and_asset_extensions() {
    assert_eq!(content_type_for("/Build/WebGL.wasm"), "application/wasm");
    assert_eq!(content_type_for("/Build/WebGL.data"), "application/octet-stream");
    assert_eq!(
        content_type_for("/Build/WebGL.framework.js.unityweb"),
        "application/octet-stream"
    );
    assert_eq!(content_type_for("/Build/WebGL.data.unityweb"), "application/octet-stream");
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
