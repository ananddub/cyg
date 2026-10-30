use super::*;
use crate::services::status::rooms::rooms;
use crate::utils::status::client_host;
use engine_bus::{RoomPhase, RoomTelemetry};

#[tokio::test]
async fn rooms_lists_the_hosted_room_with_its_ports() {
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        axum::http::header::HOST,
        "arena.example:9999".parse().expect("valid host header"),
    );
    let body = json_of(rooms(&state(), headers).await).await;
    let room = &body[0];
    for key in [
        "name",
        "clients",
        "phase",
        "maxClients",
        "ready",
        "tickHz",
        "tickRate",
        "port",
        "controlPort",
        "serverId",
        "signalingUrl",
        "udpPort",
        "hasSimSample",
        "ageMs",
    ] {
        assert!(room.get(key).is_some(), "missing {key} in {room}");
    }
    assert_eq!(room["name"].as_str(), Some("tank-3d"));
    assert_eq!(room["controlPort"].as_u64(), Some(8124));
    assert_eq!(
        room["signalingUrl"].as_str(),
        Some("ws://arena.example:8124/ws/signal")
    );
    assert_eq!(room["serverId"].as_str(), Some("http://arena.example:8124"));
    assert_eq!(room["hasSimSample"].as_bool(), Some(false));
    assert!(room["ageMs"].is_null(), "no sample means no age");
    assert!(room["phase"].is_null(), "no sample means no observed phase");
}

/// Once the host publishes its room table, `/rooms` reports each room rather
/// than collapsing them into one game-named row.
#[tokio::test]
async fn rooms_reports_every_live_room_and_its_phase() {
    let state = state();
    *state.latest_status.lock().unwrap() = Some(StatusSnapshot {
        players: 3,
        rooms: vec![
            RoomTelemetry {
                name: "arena".into(),
                phase: RoomPhase::Active,
                clients: 3,
                tick: 90,
                age_sec: 12,
                closes_in_sec: Some(288),
                ..Default::default()
            },
            RoomTelemetry {
                name: "lobby".into(),
                phase: RoomPhase::Hibernating,
                ..Default::default()
            },
        ],
        ..Default::default()
    });
    let body = json_of(rooms(&state, axum::http::HeaderMap::new()).await).await;
    let rows = body.as_array().expect("room list");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["name"], "arena");
    assert_eq!(rows[0]["phase"], "active");
    assert_eq!(rows[0]["clients"], 3);
    assert_eq!(rows[0]["tick"], 90);
    assert_eq!(rows[0]["closesInSec"], 288);
    assert_eq!(rows[1]["name"], "lobby");
    assert_eq!(rows[1]["phase"], "hibernating");
    assert!(rows[1]["closesInSec"].is_null(), "an unlimited room has no countdown");
    assert!(rows[1]["ready"].as_bool().unwrap(), "an empty room takes joins");
}

#[test]
fn a_wildcard_bind_never_leaks_into_the_dialable_host() {
    let mut headers = axum::http::HeaderMap::new();
    for wildcard in ["0.0.0.0", "::", "*"] {
        headers.insert(
            axum::http::header::HOST,
            wildcard.parse().expect("valid host header"),
        );
        assert_eq!(client_host(&headers), "127.0.0.1", "bind_addr {wildcard}");
    }
    headers.insert(
        axum::http::header::HOST,
        "game.example:8124".parse().expect("valid host header"),
    );
    assert_eq!(
        client_host(&headers),
        "game.example",
        "the advertised port wins over the header's"
    );
}
