//! A handler's failure: one status code plus the shared `{"error":{…}}` envelope,
//! so a browser can tell *why* a request was refused instead of guessing.
//!
//! The body matches `/v2`: a numeric `code` using the same gRPC convention as
//! [`crate::error::V2Error`], a `message` and a `details` list, derived from the
//! status code rather than a separate kind.

use std::borrow::Cow;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

pub struct ApiError {
    pub status: StatusCode,
    pub message: Cow<'static, str>,
    pub retry_after: Option<u32>,
}

/// The gRPC code a browser-facing status maps to, mirroring the `/v2` kinds so a
/// client sees one numbering across the API. Unknown statuses report `INTERNAL`.
fn grpc_code(status: StatusCode) -> u8 {
    match status {
        StatusCode::BAD_REQUEST => 3,
        StatusCode::UNAUTHORIZED => 16,
        StatusCode::FORBIDDEN => 7,
        StatusCode::NOT_FOUND => 5,
        StatusCode::CONFLICT => 6,
        StatusCode::TOO_MANY_REQUESTS => 8,
        StatusCode::SERVICE_UNAVAILABLE => 14,
        StatusCode::NOT_IMPLEMENTED => 12,
        _ => 13,
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: ErrorDetails,
}

#[derive(Serialize)]
struct ErrorDetails {
    code: u8,
    message: String,
    details: Vec<String>,
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
        // Serialised, not `{:?}`: a debug-escape is not JSON, so a message with a
        // quote or a non-ASCII char would reach the browser as `\"` and `\u…` text.
        let fallback = ErrorBody {
            error: ErrorDetails {
                code: 13,
                message: "internal".to_string(),
                details: Vec::new(),
            },
        };
        let body = serde_json::to_string(&ErrorBody {
            error: ErrorDetails {
                code: grpc_code(self.status),
                message: self.message.into_owned(),
                details: Vec::new(),
            },
        })
        .unwrap_or_else(|_| serde_json::to_string(&fallback).expect("static error body"));
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
