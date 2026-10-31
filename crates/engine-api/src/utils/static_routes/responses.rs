use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};

use super::mime::content_type_for;

pub(crate) fn file_response(path: &str, body: std::sync::Arc<Vec<u8>>) -> Response {
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

pub(crate) fn not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        [(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("text/plain; charset=utf-8"),
        )],
        "404 not found",
    )
        .into_response()
}
