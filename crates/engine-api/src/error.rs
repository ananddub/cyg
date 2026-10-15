//! A handler's failure: one status code plus a JSON `{"error": …}` body, so a
//! browser can tell *why* a request was refused instead of guessing.

use std::borrow::Cow;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

pub struct ApiError {
    pub status: StatusCode,
    pub message: Cow<'static, str>,
    pub retry_after: Option<u32>,
}

impl ApiError {
    pub fn new(status: StatusCode, message: impl Into<Cow<'static, str>>) -> ApiError {
        ApiError { status, message: message.into(), retry_after: None }
    }
    /// 429 with the `Retry-After` the bucket computed, so a well-behaved page
    /// backs off instead of retrying in a tight loop.
    pub fn rate_limited(retry_after_secs: u32) -> ApiError {
        ApiError {
            status: StatusCode::TOO_MANY_REQUESTS,
            message: Cow::Borrowed("matchmake rate limit exceeded"),
            retry_after: Some(retry_after_secs.max(1)),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        // Serialised, not `{:?}`: a debug-escape is not JSON, so a message with
        // a quote or a non-ASCII character in it (a room name echoed back, a
        // translated string) would reach the browser as `\"` and `\u…` text.
        let body = serde_json::to_string(&serde_json::json!({ "error": self.message }))
            .unwrap_or_else(|_| "{\"error\":\"internal\"}".to_string());
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            "application/json".parse().expect("static header value"),
        );
        if let Some(secs) = self.retry_after {
            headers.insert(
                axum::http::header::RETRY_AFTER,
                secs.to_string().parse().expect("seconds are a valid header"),
            );
        }
        (self.status, headers, body).into_response()
    }
}

#[cfg(test)]
mod tests;
