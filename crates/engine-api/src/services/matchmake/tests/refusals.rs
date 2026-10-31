use super::*;
use crate::dto::matchmake as wire;
use crate::utils::matchmake as utils;
use axum::http::StatusCode;
use axum::response::IntoResponse;

#[test]
fn an_unknown_method_is_404_and_a_bad_room_name_is_400() {
    let state = test_state(1000, 1000);
    assert_eq!(
        err(&state, ip(1), "spectateSoon", "tank-3d", "{}").status,
        StatusCode::NOT_FOUND
    );
    for room in ["", "../etc", "room name", "a\"b", "a/b", &"x".repeat(65)] {
        assert_eq!(
            err(&state, ip(1), "joinOrCreate", room, "{}").status,
            StatusCode::BAD_REQUEST,
            "room {room:?}"
        );
    }
    assert_eq!(minted(&state), 0);
}

#[test]
fn a_hostile_room_name_cannot_break_the_json_envelope() {
    let state = test_state(1000, 1000);
    for room in ["evil\"name", "evil/name", "evil\nname", "evil\\name"] {
        let resp = err(&state, ip(1), "joinOrCreate", room, "{}").into_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            resp.headers()
                .get(axum::http::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap(),
            "application/json"
        );
    }
}

#[test]
fn room_names_are_restricted() {
    assert!(valid_room_name("tank-3d"));
    assert!(valid_room_name("arena_final"));
    assert!(!valid_room_name(""));
    assert!(!valid_room_name("../etc"));
    assert!(!valid_room_name("room name"));
    assert!(!valid_room_name(&"x".repeat(65)));
}

#[test]
fn escaping_survives_a_hostile_string() {
    assert_eq!(wire::json_escape("a\"b"), "a\\\"b");
    assert_eq!(wire::json_escape("a\\b"), "a\\\\b");
    assert_eq!(wire::json_escape("a\u{1}b"), "a\\u0001b");
}

#[test]
fn json_field_reads_flat_bodies_and_ignores_noise() {
    assert_eq!(
        json_string_field("{\"reconnectionToken\":\"abc\"}", "reconnectionToken").as_deref(),
        Some("abc")
    );
    assert!(json_string_field("not json", "reconnectionToken").is_none());
    assert!(json_string_field("{}", "reconnectionToken").is_none());
    assert!(json_string_field("{\"reconnectionToken\":42}", "reconnectionToken").is_none());
}

#[test]
fn printable_keeps_error_text_json_safe() {
    assert_eq!(utils::printable("join\u{1}OrCreate"), "joinOrCreate");
    assert_eq!(utils::printable("a\"b\\c"), "a\"b\\c");
    assert_eq!(utils::printable(&"x".repeat(200)).len(), 64);
}
