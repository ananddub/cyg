use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};

pub fn html_response(body: Vec<u8>, cache: &'static str) -> Response {
    file_response(body, "text/html; charset=utf-8", cache)
}

pub fn file_response(body: Vec<u8>, mime: &'static str, cache: &'static str) -> Response {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, header::HeaderValue::from_static(mime)),
            (
                header::CACHE_CONTROL,
                header::HeaderValue::from_static(cache),
            ),
        ],
        body,
    )
        .into_response()
}

pub fn not_built(status: StatusCode) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, header::HeaderValue::from_static("text/plain; charset=utf-8"))],
        "dashboard not built yet — run `npm install && npm run build` in the dashboard/ directory, or set OX_DASHBOARD_DIST\n",
    )
        .into_response()
}

pub fn not_found() -> Response {
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
