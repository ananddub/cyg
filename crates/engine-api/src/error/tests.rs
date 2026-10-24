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
    let (status, body, retry) = body_of(ApiError::new(
        StatusCode::BAD_REQUEST,
        "invalid room name: réssort\"x\"",
    ))
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(retry.is_none(), "a plain error advertises no backoff");
    let parsed: serde_json::Value = serde_json::from_str(&body).expect("{error} is JSON");
    assert_eq!(parsed["error"]["code"], 3, "400 maps to INVALID_ARGUMENT");
    assert_eq!(
        parsed["error"]["message"],
        "invalid room name: réssort\"x\""
    );
    assert_eq!(parsed["error"]["details"], serde_json::json!([]));
}

#[tokio::test]
async fn a_rate_limit_error_carries_a_usable_retry_after() {
    let (status, body, retry) = body_of(ApiError::rate_limited(0)).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        retry.as_deref(),
        Some("1"),
        "zero seconds would tell a client to hammer again"
    );
    let parsed: serde_json::Value = serde_json::from_str(&body).expect("{error} is JSON");
    assert_eq!(parsed["error"]["code"], 8, "429 maps to RESOURCE_EXHAUSTED");
    assert!(
        parsed["error"]["message"]
            .as_str()
            .unwrap()
            .contains("rate limit")
    );
}

/// A `/v2` refusal carries its kind twice: the HTTP status and the numeric gRPC
/// `code` in the envelope, with the message beside it and no details by default.
#[tokio::test]
async fn a_v2_refusal_is_a_status_plus_an_error_envelope() {
    let response = V2Error::invalid_argument("bad thing").into_response();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("error body");
    let value: serde_json::Value = serde_json::from_slice(&bytes).expect("envelope is JSON");
    assert_eq!(value["error"]["code"], 3);
    assert_eq!(value["error"]["message"], "bad thing");
    assert_eq!(value["error"]["details"], serde_json::json!([]));
    assert_eq!(
        String::from_utf8_lossy(&bytes),
        r#"{"error":{"code":3,"message":"bad thing","details":[]}}"#
    );
}

/// The codes are the gRPC numbers the kinds are named for, and every kind that has
/// no HTTP mapping of its own still reports `INTERNAL`'s 13.
#[tokio::test]
async fn each_v2_kind_keeps_its_grpc_code() {
    for (error, code) in [
        (V2Error::not_found("gone"), 5),
        (V2Error::already_exists("clash"), 6),
        (V2Error::permission_denied("no"), 7),
        (V2Error::unauthenticated("who"), 16),
        (V2Error::no_db(), 14),
        (V2Error::internal("oops"), 13),
    ] {
        let response = error.into_response();
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .expect("error body");
        let value: serde_json::Value = serde_json::from_slice(&bytes).expect("envelope is JSON");
        assert_eq!(value["error"]["code"], code, "body: {value}");
    }
}
