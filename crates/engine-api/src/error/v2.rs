//! The `/v2` failure body: an HTTP status plus the envelope
//! `{"error":{"code":<grpc code>,"message":"…","details":[…]}}`.
//!
//! [`ErrorKind`] names the failure and chooses both the status (400 bad request, 401
//! unauthorized, 403 forbidden, 404 not found, 409 conflict, 429 rate limited, 503
//! unavailable, …) and the numeric code, so the message only has to say *why*.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

/// What kind of failure a `/v2` route reports. The kind is both the gRPC code in the
/// body and what chooses the HTTP status of the refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrorKind(u8);

pub const OK: ErrorKind = ErrorKind(0);
pub const CANCELLED: ErrorKind = ErrorKind(1);
pub const UNKNOWN: ErrorKind = ErrorKind(2);
pub const INVALID_ARGUMENT: ErrorKind = ErrorKind(3);
pub const DEADLINE_EXCEEDED: ErrorKind = ErrorKind(4);
pub const NOT_FOUND: ErrorKind = ErrorKind(5);
pub const ALREADY_EXISTS: ErrorKind = ErrorKind(6);
pub const PERMISSION_DENIED: ErrorKind = ErrorKind(7);
pub const RESOURCE_EXHAUSTED: ErrorKind = ErrorKind(8);
pub const FAILED_PRECONDITION: ErrorKind = ErrorKind(9);
pub const ABORTED: ErrorKind = ErrorKind(10);
pub const OUT_OF_RANGE: ErrorKind = ErrorKind(11);
pub const UNIMPLEMENTED: ErrorKind = ErrorKind(12);
pub const INTERNAL: ErrorKind = ErrorKind(13);
pub const UNAVAILABLE: ErrorKind = ErrorKind(14);
pub const DATA_LOSS: ErrorKind = ErrorKind(15);
pub const UNAUTHENTICATED: ErrorKind = ErrorKind(16);

impl ErrorKind {
    /// The gRPC status code this kind is named for, sent as `error.code`.
    pub(crate) fn code(self) -> u8 {
        self.0
    }

    /// The HTTP status a refusal of this kind answers with.
    pub(crate) fn status(self) -> StatusCode {
        match self {
            OK => StatusCode::OK,
            INVALID_ARGUMENT | FAILED_PRECONDITION | OUT_OF_RANGE => StatusCode::BAD_REQUEST,
            UNAUTHENTICATED => StatusCode::UNAUTHORIZED,
            PERMISSION_DENIED => StatusCode::FORBIDDEN,
            NOT_FOUND => StatusCode::NOT_FOUND,
            ALREADY_EXISTS | ABORTED => StatusCode::CONFLICT,
            RESOURCE_EXHAUSTED => StatusCode::TOO_MANY_REQUESTS,
            UNAVAILABLE => StatusCode::SERVICE_UNAVAILABLE,
            UNIMPLEMENTED => StatusCode::NOT_IMPLEMENTED,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

/// The SQLSTATE for a unique-constraint violation — the one database error whose
/// meaning a client can act on, so handlers test for it by name and constraint.
const UNIQUE_VIOLATION: &str = "23505";

#[derive(Debug)]
pub struct V2Error {
    pub kind: ErrorKind,
    pub message: String,
}

impl V2Error {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> V2Error {
        V2Error {
            kind,
            message: message.into(),
        }
    }
    pub fn invalid_argument(message: impl Into<String>) -> V2Error {
        V2Error::new(INVALID_ARGUMENT, message)
    }
    pub fn unauthenticated(message: impl Into<String>) -> V2Error {
        V2Error::new(UNAUTHENTICATED, message)
    }
    pub fn not_found(message: impl Into<String>) -> V2Error {
        V2Error::new(NOT_FOUND, message)
    }
    pub fn already_exists(message: impl Into<String>) -> V2Error {
        V2Error::new(ALREADY_EXISTS, message)
    }
    pub fn permission_denied(message: impl Into<String>) -> V2Error {
        V2Error::new(PERMISSION_DENIED, message)
    }
    pub fn failed_precondition(message: impl Into<String>) -> V2Error {
        V2Error::new(FAILED_PRECONDITION, message)
    }
    pub fn unimplemented(message: impl Into<String>) -> V2Error {
        V2Error::new(UNIMPLEMENTED, message)
    }
    /// Facebook Instant Game uses this for an unverifiable player signature: the
    /// server cannot make sense of it and retrying will not help.
    pub fn data_loss(message: impl Into<String>) -> V2Error {
        V2Error::new(DATA_LOSS, message)
    }
    pub fn internal(message: impl Into<String>) -> V2Error {
        V2Error::new(INTERNAL, message)
    }
    /// The database is the whole `/v2` surface; without it every endpoint is
    /// unavailable, and that is stated rather than faked.
    pub fn no_db() -> V2Error {
        V2Error::new(
            UNAVAILABLE,
            "The database is not configured, set [control.session] database_url.",
        )
    }
    /// A missing row is the one database error that is a client's problem; every
    /// other one leaks an internal message the server should not repeat, so it
    /// becomes a bare Internal like the `logger.Error(...)` paths.
    pub fn from_sqlx(e: sqlx::Error) -> V2Error {
        match e {
            sqlx::Error::RowNotFound => V2Error::not_found("Entity not found."),
            other => {
                // The detail is logged only; the client gets the opaque body BaaS returns for a database error.
                tracing::error!("[v2] database error: {other}");
                V2Error::internal("Internal server error.")
            }
        }
    }
    /// True when `e` is a unique violation on `constraint` — BaaS distinguishes
    /// `"users_username_key"` from `"users_email_key"` and answers each differently.
    pub fn is_unique_violation(e: &sqlx::Error, constraint: &str) -> bool {
        Self::is_any_unique_violation(e)
            && e.as_database_error()
                .is_some_and(|db| db.message().contains(constraint))
    }

    /// A unique violation whatever the index is named. Where the clash is on a
    /// table the client cannot address — a device row's primary key — that is all
    /// BaaS tests before it reports a conflict.
    pub fn is_any_unique_violation(e: &sqlx::Error) -> bool {
        e.as_database_error()
            .is_some_and(|db| db.code().as_deref() == Some(UNIQUE_VIOLATION))
    }
}

/// The failure envelope `/v2` writes on the wire: `{"error":{…}}`. Serialising a
/// struct rather than a `json!` map keeps the `code`, `message`, `details` order.
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

impl IntoResponse for V2Error {
    fn into_response(self) -> Response {
        let status = self.kind.status();
        let body = axum::Json(ErrorBody {
            error: ErrorDetails {
                code: self.kind.code(),
                message: self.message,
                details: Vec::new(),
            },
        });
        let mut response = (status, body).into_response();
        if status == StatusCode::UNAUTHORIZED {
            // Every 401 carries this realm, and the refusal text must not leak into the header.
            response.headers_mut().insert(
                axum::http::header::WWW_AUTHENTICATE,
                axum::http::HeaderValue::from_static("Bearer realm=\"cyg\""),
            );
        }
        response
    }
}

impl From<sqlx::Error> for V2Error {
    fn from(e: sqlx::Error) -> V2Error {
        V2Error::from_sqlx(e)
    }
}
