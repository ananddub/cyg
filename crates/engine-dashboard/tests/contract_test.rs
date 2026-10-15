//! Pins the dashboard wire contract so the frontend cannot drift.
//!
//! This test only uses the public surface of `engine_dashboard`: it is the
//! contract, not an implementation detail. If a struct here stops serialising
//! camelCase, or a histogram stops being `TICK_HISTOGRAM_BUCKETS` long, the
//! frontend's `types.ts` and every live graph silently desync — which is exactly
//! what this file exists to catch.

use serde_json::Value;

use engine_dashboard::{
    BusHealth, Capacity, ClientRow, DashboardHistory, Overview, RateLimitHealth, RoomRow, ServerInfo,
    SeatHealth, StatusSnapshot, TickHealth, TickPoint, TickSeries, TransportMix, WorkerRow, WsFrame,
    HISTORY_LEN, TICK_HISTOGRAM_BUCKETS, WS_PROTOCOL, bucket_edges_us, histogram_add,
};

/// Walk a serialised value and reject any object key that is not camelCase —
/// the single rule that lets `dashboard/src/lib/types.ts` and this crate agree.
/// Serde emits every field regardless of its value, so default-constructed
/// structs still expose the whole key set to this check.
fn assert_camel_case(value: &Value, path: &str) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                assert!(!key.contains('_'), "snake_case key `{key}` at {path} breaks the camelCase contract");
                assert_camel_case(child, &format!("{path}/{key}"));
            }
        }
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                assert_camel_case(item, &format!("{path}[{i}]"));
            }
        }
        _ => {}
    }
}

/// Serialise one contract type and assert its whole tree is camelCase.
fn camel<T: serde::Serialize>(name: &str, value: &T) -> Value {
    let json = serde_json::to_value(value).unwrap_or_else(|e| panic!("{name} failed to serialise: {e}"));
    assert_camel_case(&json, name);
    json
}

#[test]
fn every_contract_struct_serialises_camel_case() {
    // `Overview` nests every other `*Health`/`*Info` group, so walking it alone
    // already recurses them; the standalone calls document that each type is
    // individually covered, and catch a type that stops being reachable.
    camel("Overview", &Overview::default());
    camel("ServerInfo", &ServerInfo::default());
    camel("Capacity", &Capacity::default());
    camel("TickHealth", &TickHealth::default());
    camel("TransportMix", &TransportMix::default());
    camel("BusHealth", &BusHealth::default());
    camel("SeatHealth", &SeatHealth::default());
    camel("RateLimitHealth", &RateLimitHealth::default());
    camel("RoomRow", &RoomRow::default());
    camel("WorkerRow", &WorkerRow::default());
    camel("ClientRow", &ClientRow::default());
    camel("TickSeries", &TickSeries::default());
    camel("TickPoint", &TickPoint::default());
    // `StatusSnapshot` and `SessionTelemetry` are the bus-level sim sample
    // (engine_bus), not dashboard wire structs: `/status` exposes `StatusSnapshot`
    // with its own snake_case keys on purpose. The camelCase contract covers only
    // the twelve types `/api/dashboard/*` and `/ws/dashboard` actually put down.
}

#[test]
fn a_populated_overview_is_still_camel_case() {
    // Non-default values exercise nested `Option`s, the passthrough `agones`
    // object and the histogram array — the branches a default can hide.
    let mut tick = TickHealth::default();
    let mut hist = [0u32; TICK_HISTOGRAM_BUCKETS];
    histogram_add(&mut hist, 900);
    tick.histogram = hist;
    let overview = Overview {
        server: ServerInfo { name: "tank-3d".into(), port: 7777, control_port: 8124, web_port: 7779 },
        ccu: 3,
        rooms: 1,
        capacity: Capacity { players: 3, max: 8 },
        tick,
        transport: TransportMix { websocket: 2, webrtc: 1, native_udp: 0, signal_peers: 4, ..Default::default() },
        agones: serde_json::json!({ "enabled": false, "consecutiveFailures": 0 }),
        has_sim_sample: true,
        ..Default::default()
    };
    let json = camel("Overview(populated)", &overview);
    // The passthrough must keep the frontend's camelCase too — an agent cannot
    // smuggle a snake_case Agones key in behind this crate.
    assert_eq!(json["agones"]["consecutiveFailures"].as_u64(), Some(0));
}

#[test]
fn histogram_and_bucket_edges_are_exactly_the_contract_length() {
    let series = camel("TickSeries(edges)", &TickSeries::default());
    assert_eq!(
        series["histogram"].as_array().unwrap().len(),
        TICK_HISTOGRAM_BUCKETS,
        "the bar chart axis must match TICK_HISTOGRAM_BUCKETS"
    );
    assert_eq!(series["bucketEdgesUs"].as_array().unwrap().len(), TICK_HISTOGRAM_BUCKETS);

    let tick = camel("TickHealth(edges)", &TickHealth::default());
    assert_eq!(tick["histogram"].as_array().unwrap().len(), TICK_HISTOGRAM_BUCKETS);

    // The helper defines the base-2 axis: edges are 2, 4, … and the last names
    // the open-ended top bucket at 4096 µs (one tick's budget at 128 Hz).
    let edges = bucket_edges_us();
    assert_eq!(edges.len(), TICK_HISTOGRAM_BUCKETS);
    assert_eq!(edges[0], 2, "first bucket covers [2,4)");
    assert_eq!(edges[1], 4);
    assert_eq!(edges[TICK_HISTOGRAM_BUCKETS - 1], 4096, "top edge names the open-ended bucket");
    for (i, edge) in edges.iter().enumerate() {
        assert_eq!(*edge, 1u64 << (i + 1), "bucket {i} edge must be 2^{}", i + 1);
    }

    // A `TickSeries` carrying the real edges — which is what `build_ticks` must
    // assign — ships exactly those values, so the client can label the bars
    // without redoing the arithmetic. A *default* series is zeroed, so it proves
    // only the length here; the value comes from the helper a handler calls.
    let wired = camel("TickSeries(wired)", &TickSeries { bucket_edges_us: edges, ..Default::default() });
    let shipped: Vec<u64> =
        wired["bucketEdgesUs"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();
    assert_eq!(shipped, edges.to_vec());

    // A default `TickSeries` must still name its buckets, or the chart renders an
    // empty axis with no scale.
    assert_eq!(TickSeries::default().bucket_edges_us.len(), TICK_HISTOGRAM_BUCKETS);
}

#[test]
fn histogram_add_lands_in_the_right_bucket() {
    // 1 -> bucket 0, 2 -> bucket 1, and everything at or past the top edge
    // (4096 µs) piles into the last bucket rather than panicking.
    for (micros, expected) in [(1u64, 0usize), (2, 1)] {
        let mut h = [0u32; TICK_HISTOGRAM_BUCKETS];
        histogram_add(&mut h, micros);
        assert_eq!(h[expected], 1, "{micros} µs must land in bucket {expected}");
        let total: u32 = h.iter().sum();
        assert_eq!(total, 1, "one add must move exactly one count");
    }

    let mut top = [0u32; TICK_HISTOGRAM_BUCKETS];
    histogram_add(&mut top, 4096);
    histogram_add(&mut top, 5000);
    assert_eq!(top[TICK_HISTOGRAM_BUCKETS - 1], 2, "4096 and 5000 both belong in the open top bucket");
    assert_eq!(top.iter().sum::<u32>(), 2);
}

#[test]
fn ws_frame_tags_are_hello_snapshot_goodbye() {
    let hello = WsFrame::Hello {
        overview: Box::new(Overview::default()),
        ticks: Box::new(TickSeries::default()),
        protocol: WS_PROTOCOL,
    };
    let snapshot = WsFrame::Snapshot {
        overview: Box::new(Overview::default()),
        ticks: Box::new(TickSeries::default()),
    };
    let goodbye = WsFrame::Goodbye { reason: "plane shutting down".into() };

    let h = camel("WsFrame(hello)", &hello);
    assert_eq!(h["type"].as_str(), Some("hello"));
    assert_eq!(h["protocol"].as_u64(), Some(WS_PROTOCOL as u64));
    assert!(h["overview"].is_object() && h["ticks"].is_object());

    let s = camel("WsFrame(snapshot)", &snapshot);
    assert_eq!(s["type"].as_str(), Some("snapshot"));
    assert!(s["overview"].is_object() && s["ticks"].is_object());

    let g = camel("WsFrame(goodbye)", &goodbye);
    assert_eq!(g["type"].as_str(), Some("goodbye"));
    assert_eq!(g["reason"].as_str(), Some("plane shutting down"));
}

#[test]
fn dashboard_history_evicts_the_oldest_past_capacity() {
    let ring = DashboardHistory::new();
    assert!(ring.is_empty());
    assert_eq!(ring.len(), 0);
    assert!(ring.newest().is_none());

    // Push past capacity; seq is the identity so eviction order is observable.
    let overflow = HISTORY_LEN + 50;
    for i in 0..overflow {
        ring.push(StatusSnapshot { seq: i as u64, ..Default::default() }, i as u64);
    }

    assert_eq!(ring.len(), HISTORY_LEN, "the ring must cap at HISTORY_LEN");
    assert!(!ring.is_empty());

    let newest = ring.newest().unwrap();
    assert_eq!(newest.snapshot.seq, (overflow - 1) as u64, "newest wins");
    assert_eq!(newest.arrived_ms, (overflow - 1) as u64);

    let all = ring.snapshot_all();
    assert_eq!(all.len(), HISTORY_LEN);
    assert_eq!(all.first().unwrap().snapshot.seq, 50, "the oldest 50 samples were evicted");
    assert_eq!(all.last().unwrap().snapshot.seq, (overflow - 1) as u64);
    // Oldest-to-newest ordering, so a chart never has to sort.
    for w in all.windows(2) {
        assert!(w[0].snapshot.seq < w[1].snapshot.seq);
    }

    ring.clear();
    assert!(ring.is_empty());
    assert!(ring.newest().is_none());
}
