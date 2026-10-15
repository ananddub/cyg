use engine_dashboard::StatusSnapshot;

use super::state;
use crate::services::dashboard::{build_overview, build_rooms, build_workers};

#[test]
fn an_idle_overview_serialises_every_key_as_zero_not_omission() {
    let s = state();
    let idle = serde_json::to_value(engine_dashboard::Overview::default()).unwrap();
    let json = serde_json::to_value(build_overview(&s)).unwrap();
    for (key, _) in idle.as_object().unwrap() {
        assert!(json.get(key).is_some(), "missing `{key}` in {json}");
    }
    assert_eq!(json["server"]["name"].as_str(), Some("tank-3d"));
    assert_eq!(json["server"]["controlPort"].as_u64(), Some(8124));
    assert_eq!(json["ccu"].as_u64(), Some(0));
    assert_eq!(json["tick"]["hz"].as_f64(), Some(0.0));
    assert_eq!(json["hasSimSample"].as_bool(), Some(false));
    assert_eq!(json["sampleAgeMs"].as_u64(), Some(0));

    let rooms = build_rooms(&s);
    assert_eq!(rooms.len(), 1, "one room per process");
    assert_eq!(rooms[0].status, "empty");
    assert!(rooms[0].sample_age_ms.is_none(), "no sample means no age, not zero");
    assert_eq!(rooms[0].server_id, "http://127.0.0.1:8124");
    assert_eq!(rooms[0].signaling_url, "ws://127.0.0.1:8124/ws/signal");

    let workers = build_workers(&s);
    assert_eq!(workers[0].name, "sim");
    assert!(!workers[0].live, "the sim has never published");
    assert!(workers[0].tick_p50_us.is_none());
    assert_eq!(workers[1].name, "control");
    assert!(workers[1].live);
}

#[test]
fn a_fed_snapshot_makes_the_room_live_with_an_age() {
    let s = state();
    *s.latest_status.lock().unwrap() = Some(StatusSnapshot {
        seq: 7,
        players: 2,
        empty_for_sec: 41,
        updated_ms: engine_bus::clock::mono_ns() / 1_000_000,
        ..Default::default()
    });
    let rooms = build_rooms(&s);
    assert_eq!(rooms[0].status, "live");
    assert_eq!(rooms[0].clients, 2);
    assert_eq!(rooms[0].empty_for_sec, None, "a room with players is not empty");
    assert!(rooms[0].has_sim_sample);
    assert!(rooms[0].sample_age_ms.unwrap() < 5_000);
    assert_eq!(build_overview(&s).ccu, 2);
    assert!(build_workers(&s)[0].live);
}
