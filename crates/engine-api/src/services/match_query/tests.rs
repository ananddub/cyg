use axum::Json;
use uuid::Uuid;

use crate::dto::matches::ListMatchesParams;
use crate::error::INVALID_ARGUMENT;
use crate::infra::ws::envelope::Presence;
use crate::services::realtime::{RealtimeMatchService, matches::MatchSnapshot};

use super::MatchQueryService;

fn presence(user: &str, session: &str) -> Presence {
    Presence {
        user_id: user.to_owned(),
        session_id: session.to_owned(),
        username: format!("player-{user}"),
        node: None,
    }
}

#[test]
fn a_labeled_snapshot_prints_the_full_api_match() {
    let snap = MatchSnapshot {
        id: Uuid::nil(),
        label: "lobby".to_owned(),
        open: true,
        tick_rate: 15,
        members: vec![presence("u1", "s1"), presence("u2", "s2")],
    };
    let value = serde_json::to_value(crate::dto::matches::MatchResponse::from(&snap)).unwrap();
    assert_eq!(value["match_id"], Uuid::nil().simple().to_string());
    assert_eq!(value["authoritative"], false);
    assert_eq!(value["label"], "lobby");
    assert_eq!(value["size"], 2);
    assert_eq!(value["tick_rate"], 15);
    assert!(value.get("handler_name").is_none());
}

#[test]
fn an_unlabeled_match_omits_the_label_and_a_zero_tick_omits_the_rate() {
    let snap = MatchSnapshot {
        id: Uuid::new_v4(),
        label: String::new(),
        open: false,
        tick_rate: 0,
        members: Vec::new(),
    };
    let value = serde_json::to_value(crate::dto::matches::MatchResponse::from(&snap)).unwrap();
    assert!(value.get("label").is_none());
    assert!(value.get("tick_rate").is_none());
    assert_eq!(value["size"], 0);
}

#[test]
fn the_state_of_a_relay_match_carries_members_and_empty_handler_fields() {
    let state = crate::dto::console::matches::ConsoleMatchState {
        presences: vec![presence("u1", "s1")],
        tick: 0,
        state: String::new(),
    };
    let value = serde_json::to_value(&state).unwrap();
    assert_eq!(value["presences"].as_array().unwrap().len(), 1);
    assert_eq!(value["presences"][0]["user_id"], "u1");
    assert_eq!(value["tick"], 0);
    assert_eq!(value["state"], "");
}

#[test]
fn a_limit_out_of_range_is_a_bad_request_before_any_read() {
    let service = MatchQueryService;
    for limit in [0i64, 101] {
        let err = service
            .list_matches(ListMatchesParams {
                limit: Some(limit),
                ..Default::default()
            })
            .unwrap_err();
        assert_eq!(err.kind, INVALID_ARGUMENT);
        assert_eq!(
            err.message,
            "Invalid limit - limit must be between 1 and 100."
        );
    }
}

#[test]
fn the_size_bounds_are_validated_against_the_reference_wording() {
    let service = MatchQueryService;
    let err = service
        .list_matches(ListMatchesParams {
            min_size: Some(-1),
            ..Default::default()
        })
        .unwrap_err();
    assert_eq!(err.message, "Minimum size must be 0 or above.");

    let err = service
        .list_matches(ListMatchesParams {
            min_size: Some(5),
            max_size: Some(2),
            ..Default::default()
        })
        .unwrap_err();
    assert_eq!(
        err.message,
        "Maximum size must be greater than or equal to minimum size when both are specified."
    );
}

#[test]
fn a_default_list_reads_the_registry_and_answers_an_array() {
    let service = MatchQueryService;
    let Json(body) = service.list_matches(ListMatchesParams::default()).unwrap();
    assert!(body.matches.iter().all(|m| !m.authoritative));
}

#[test]
fn a_created_match_is_listed_by_id_and_a_terminated_match_is_gone() {
    let id = RealtimeMatchService::create_match_with_tokens(15, vec!["token".to_owned()]);
    assert!(RealtimeMatchService::get_snapshot(id).is_some_and(|s| s.tick_rate == 15));

    let Json(body) = MatchQueryService
        .list_matches(ListMatchesParams {
            limit: Some(100),
            ..Default::default()
        })
        .unwrap();
    assert!(
        body.matches
            .iter()
            .any(|m| m.match_id == id.simple().to_string()),
        "a running match must appear in the list"
    );

    assert!(RealtimeMatchService::terminate_match(id));
    assert!(RealtimeMatchService::get_snapshot(id).is_none());
    assert!(
        !RealtimeMatchService::terminate_match(id),
        "terminating a gone match is a second no-op"
    );
}

#[test]
fn matchmaker_stats_reports_the_real_queue_shape() {
    let Json(stats) = MatchQueryService.matchmaker_stats();
    let value = serde_json::to_value(&stats).unwrap();
    assert!(value["ticket_count"].is_number());
    assert!(value.get("completions").is_none() || value["completions"].is_array());
}
