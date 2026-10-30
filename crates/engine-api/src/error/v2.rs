use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

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
    pub(crate) fn code(self) -> u8 {
        self.0
    }

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
    pub fn data_loss(message: impl Into<String>) -> V2Error {
        V2Error::new(DATA_LOSS, message)
    }
    pub fn internal(message: impl Into<String>) -> V2Error {
        V2Error::new(INTERNAL, message)
    }
    pub fn unavailable(message: impl Into<String>) -> V2Error {
        V2Error::new(UNAVAILABLE, message)
    }
    pub fn no_db() -> V2Error {
        V2Error::new(
            UNAVAILABLE,
            "The database is not configured, set [control.session] database_endpoint.",
        )
    }
    pub fn no_blobs() -> V2Error {
        V2Error::new(
            UNAVAILABLE,
            "The blob store is not configured, set [control.session] blobs.",
        )
    }
    pub fn is_unique_violation(e: &crate::db::DbError, index: &str) -> bool {
        e.unique_index() == Some(index)
    }

    pub fn is_any_unique_violation(e: &crate::db::DbError) -> bool {
        e.is_unique()
    }
}

impl From<crate::db::BlobError> for V2Error {
    fn from(e: crate::db::BlobError) -> V2Error {
        use crate::db::BlobError;
        match e {
            BlobError::Unconfigured => V2Error::no_blobs(),
            BlobError::Config(message) => {
                tracing::error!("[v2] blob store misconfigured: {message}");
                V2Error::no_blobs()
            }
            BlobError::InvalidKey(message) => {
                tracing::error!("[v2] invalid blob key: {message}");
                V2Error::internal("Internal server error.")
            }
            BlobError::Io(message) | BlobError::Bucket(message) => {
                tracing::error!("[v2] blob store error: {message}");
                V2Error::internal("Internal server error.")
            }
        }
    }
}

impl From<crate::db::DbError> for V2Error {
    fn from(e: crate::db::DbError) -> V2Error {
        use crate::db::DbError;
        match e {
            DbError::NotFound => V2Error::not_found("Entity not found."),
            DbError::Unique(_) => V2Error::already_exists("Entity already exists."),
            DbError::Connection(message) => {
                tracing::error!("[v2] database unavailable: {message}");
                V2Error::new(UNAVAILABLE, "The database is unavailable.")
            }
            DbError::Query(message) | DbError::Deserialize(message) => {
                tracing::error!("[v2] database error: {message}");
                V2Error::internal("Internal server error.")
            }
        }
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
            response.headers_mut().insert(
                axum::http::header::WWW_AUTHENTICATE,
                axum::http::HeaderValue::from_static("Bearer realm=\"cyg\""),
            );
        }
        response
    }
}
