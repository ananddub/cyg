use std::borrow::Cow;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub message: Cow<'static, str>,
    pub retry_after: Option<u32>,
}

/// The canonical code for a status.
///
/// The legacy envelope and the `/v2` envelope must agree on what a status
/// means, so this asks the `/v2` table instead of keeping a second copy of the
/// mapping. Only kinds that own a status arm are consulted: kinds that reach 500
/// merely as the table's fallback would otherwise make a 500 read as
/// `DEADLINE_EXCEEDED`. Anything the table does not name is `INTERNAL`.
fn grpc_code(status: StatusCode) -> u8 {
    use super::v2::{
        ALREADY_EXISTS, INTERNAL, INVALID_ARGUMENT, NOT_FOUND, PERMISSION_DENIED,
        RESOURCE_EXHAUSTED, UNAUTHENTICATED, UNAVAILABLE, UNIMPLEMENTED,
    };
    [
        INVALID_ARGUMENT,
        UNAUTHENTICATED,
        PERMISSION_DENIED,
        NOT_FOUND,
        ALREADY_EXISTS,
        RESOURCE_EXHAUSTED,
        UNIMPLEMENTED,
        UNAVAILABLE,
    ]
    .into_iter()
    .find(|kind| kind.status() == status)
    .map(|kind| kind.code())
    .unwrap_or(INTERNAL.code())
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
        ApiError {
            status,
            message: message.into(),
            retry_after: None,
        }
    }
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
                secs.to_string()
                    .parse()
                    .expect("seconds are a valid header"),
            );
        }
        (self.status, headers, body).into_response()
    }
}
