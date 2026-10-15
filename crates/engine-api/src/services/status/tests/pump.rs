use super::*;
use crate::infra::pump::pump;
use crate::services::status::rooms::rooms;
use crate::services::status::snapshot::status;

/// The pump is the only consumer of the status queue and keeps the newest of
/// several queued samples.
#[tokio::test]
async fn pump_keeps_the_newest_snapshot_the_sim_publishes() {
    let s = state();
    let pump_task = tokio::spawn(pump(s.clone()));
    // Let the pump run to its first await: it takes the queue there, and
    // probing `try_take()` from here would steal it out from under it.
    for _ in 0..4 {
        tokio::task::yield_now().await;
    }

    let now_ms = mono_ns() / 1_000_000;
    s.bus
        .status_tx
        .try_send(StatusSnapshot {
            seq: 1,
            ..Default::default()
        })
        .unwrap();
    s.bus
        .status_tx
        .try_send(StatusSnapshot {
            seq: 2,
            players: 3,
            tick_hz_est: 128.0,
            updated_ms: now_ms,
            ..Default::default()
        })
        .unwrap();

    for _ in 0..400 {
        if s.latest_status
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|x| x.seq == 2)
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    pump_task.abort();

    let held = s
        .latest_status
        .lock()
        .unwrap()
        .clone()
        .expect("pump stored the sample");
    assert_eq!(held.seq, 2, "the newest sample must win");
    assert_eq!(held.players, 3);

    // A second pump cannot double-consume the queue: `/status` degrades to
    // the last sample rather than erroring.
    assert!(s.status_rx.try_take().is_none(), "the pump owns the queue");

    let status_body = json_of(status(&s.clone()).await).await;
    assert_eq!(status_body["sim"]["seq"].as_u64(), Some(2));
    assert_eq!(status_body["sim"]["players"].as_u64(), Some(3));
    assert!(status_body["sim"]["age_ms"].as_u64().unwrap() < 5_000);

    // `/api/rooms` reads the same sample, so occupancy and telemetry agree.
    let rooms_body = json_of(rooms(&s, axum::http::HeaderMap::new()).await).await;
    assert_eq!(rooms_body[0]["clients"].as_u64(), Some(3));
    assert_eq!(rooms_body[0]["tickHz"].as_f64(), Some(128.0));
    assert_eq!(rooms_body[0]["hasSimSample"].as_bool(), Some(true));
    assert!(rooms_body[0]["ageMs"].as_u64().unwrap() < 5_000);
}
