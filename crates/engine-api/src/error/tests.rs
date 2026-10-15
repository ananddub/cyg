use axum::http::StatusCode;
use axum::response::IntoResponse;

use super::{ApiError, V2Error};

async fn body_of(err: ApiError) -> (StatusCode, String, Option<String>) {
    let response = err.into_response();
    let status = response.status();
    let retry = response
        .headers()
        .get(axum::http::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("error body");
    (status, String::from_utf8_lossy(&bytes).into_owned(), retry)
}

/// A refusal has to be readable, and the message often quotes text the caller
/// sent — a room name or a method segment. Debug-escaping that into JSON is how a
/// browser ends up parsing `"rooms\u00e9"`.
#[tokio::test]
async fn an_error_body_is_json_even_when_the_message_is_not_ascii() {
    let (status, body, retry) =
        body_of(ApiError::new(StatusCode::BAD_REQUEST, "invalid room name: réssort\"x\"")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(retry.is_none(), "a plain error advertises no backoff");
    let parsed: serde_json::Value = serde_json::from_str(&body).expect("{error} is JSON");
    assert_eq!(parsed["error"], "invalid room name: réssort\"x\"");
}

#[tokio::test]
async fn a_rate_limit_error_carries_a_usable_retry_after() {
    let (status, body, retry) = body_of(ApiError::rate_limited(0)).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(retry.as_deref(), Some("1"), "zero seconds would tell a client to hammer again");
    let parsed: serde_json::Value = serde_json::from_str(&body).expect("{error} is JSON");
    assert!(parsed["error"].is_string());
}

/// A `/v2` refusal names its kind through the HTTP status, so the body carries the
/// message and nothing else — no numeric `code` for a client to branch on.
#[tokio::test]
async fn a_v2_refusal_is_a_status_plus_a_bare_message() {
    let response = V2Error::invalid_argument("bad thing").into_response();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("error body");
    assert_eq!(
        std::str::from_utf8(&bytes).expect("utf-8 body"),
        r#"{"message":"bad thing"}"#
    );
}
