use engine_bus::SessionTelemetry;
use engine_dashboard::{MAX_CLIENT_ROWS, StatusSnapshot};

use super::state;
use crate::services::dashboard::build_clients;

#[test]
fn client_rows_map_the_newest_sessions_in_order_and_cap() {
    let _guard = super::TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let h = crate::utils::dashboard::history::history();
    let total = MAX_CLIENT_ROWS + 50;
    let sessions: Vec<SessionTelemetry> = (0..total)
        .map(|i| SessionTelemetry {
            session: format!("cap-{i}"),
            transport: "websocket".into(),
            player_id: Some(i as i64),
            acked_seq: Some(i as u32),
            idle_ms: Some(i as u64),
        })
        .collect();
    let s = state();
    let mut rows = Vec::new();
    for _ in 0..3 {
        h.clear();
        h.push(
            StatusSnapshot { seq: 9500, players: total as u32, sessions: sessions.clone(), ..Default::default() },
            engine_bus::clock::mono_ns() / 1_000_000,
        );
        rows = build_clients(&s);
        if rows.len() == MAX_CLIENT_ROWS {
            break;
        }
    }
    assert_eq!(rows.len(), MAX_CLIENT_ROWS, "a runaway registry cannot size the response");
    assert_eq!(rows[0].session, "cap-0");
    assert_eq!(rows.last().unwrap().session, format!("cap-{}", MAX_CLIENT_ROWS - 1), "in order");
    assert!(rows.iter().all(|r| r.room == "tank-3d" && r.transport == "websocket" && r.idle_ms.is_some()));
}
