use engine_dashboard::{
    DashboardHistory, HISTORY_LEN, Overview, StatusSnapshot, TICK_HISTOGRAM_BUCKETS, TickHealth,
    TickSeries, WS_PROTOCOL, WsFrame, bucket_edges_us, histogram_add,
};

#[test]
fn histogram_and_bucket_edges_are_exactly_the_contract_length() {
    let series = serde_json::to_value(TickSeries::default()).unwrap();
    assert_eq!(
        series["histogram"].as_array().unwrap().len(),
        TICK_HISTOGRAM_BUCKETS
    );
    assert_eq!(
        series["bucketEdgesUs"].as_array().unwrap().len(),
        TICK_HISTOGRAM_BUCKETS
    );
    let tick = serde_json::to_value(TickHealth::default()).unwrap();
    assert_eq!(
        tick["histogram"].as_array().unwrap().len(),
        TICK_HISTOGRAM_BUCKETS
    );

    let edges = bucket_edges_us();
    assert_eq!(edges.len(), TICK_HISTOGRAM_BUCKETS);
    assert_eq!(edges[0], 2);
    assert_eq!(edges[1], 4);
    assert_eq!(
        edges[TICK_HISTOGRAM_BUCKETS - 1],
        4096,
        "top edge names the open-ended bucket"
    );
    for (i, edge) in edges.iter().enumerate() {
        assert_eq!(
            *edge,
            1u64 << (i + 1),
            "bucket {i} edge must be 2^{}",
            i + 1
        );
    }
}

#[test]
fn histogram_add_lands_in_the_right_bucket() {
    for (micros, expected) in [(1u64, 0usize), (2, 1)] {
        let mut h = [0u32; TICK_HISTOGRAM_BUCKETS];
        histogram_add(&mut h, micros);
        assert_eq!(h[expected], 1, "{micros} µs must land in bucket {expected}");
        assert_eq!(
            h.iter().sum::<u32>(),
            1,
            "one add must move exactly one count"
        );
    }

    let mut top = [0u32; TICK_HISTOGRAM_BUCKETS];
    histogram_add(&mut top, 4096);
    histogram_add(&mut top, 5000);
    assert_eq!(
        top[TICK_HISTOGRAM_BUCKETS - 1],
        2,
        "4096 and 5000 both belong in the open top bucket"
    );
    assert_eq!(top.iter().sum::<u32>(), 2);
}

#[test]
fn ws_frame_tags_are_hello_snapshot_live_goodbye() {
    let hello = WsFrame::Hello {
        overview: Box::new(Overview::default()),
        ticks: Box::new(TickSeries::default()),
        protocol: WS_PROTOCOL,
    };
    let snapshot = WsFrame::Snapshot {
        overview: Box::new(Overview::default()),
        ticks: Box::new(TickSeries::default()),
    };
    let goodbye = WsFrame::Goodbye {
        reason: "plane shutting down".into(),
    };
    let live = WsFrame::Live {
        topic: "users".into(),
        action: "create".into(),
        record: "abc".into(),
        seq: 7,
    };

    let h = serde_json::to_value(&hello).unwrap();
    assert_eq!(h["type"].as_str(), Some("hello"));
    assert_eq!(h["protocol"].as_u64(), Some(WS_PROTOCOL as u64));
    assert!(h["overview"].is_object() && h["ticks"].is_object());
    assert_eq!(
        serde_json::to_value(&snapshot).unwrap()["type"].as_str(),
        Some("snapshot")
    );
    let l = serde_json::to_value(&live).unwrap();
    assert_eq!(l["type"].as_str(), Some("live"));
    assert!(
        l.get("overview").is_none() && l.get("ticks").is_none(),
        "a live frame is an invalidation, not a snapshot"
    );
    let g = serde_json::to_value(&goodbye).unwrap();
    assert_eq!(g["type"].as_str(), Some("goodbye"));
    assert_eq!(g["reason"].as_str(), Some("plane shutting down"));
}

#[test]
fn dashboard_history_evicts_the_oldest_past_capacity() {
    let ring = DashboardHistory::new();
    assert!(ring.is_empty());
    assert!(ring.newest().is_none());

    let overflow = HISTORY_LEN + 50;
    for i in 0..overflow {
        ring.push(
            StatusSnapshot {
                seq: i as u64,
                ..Default::default()
            },
            i as u64,
        );
    }

    assert_eq!(ring.len(), HISTORY_LEN, "the ring must cap at HISTORY_LEN");
    let newest = ring.newest().unwrap();
    assert_eq!(newest.snapshot.seq, (overflow - 1) as u64, "newest wins");

    let all = ring.snapshot_all();
    assert_eq!(
        all.first().unwrap().snapshot.seq,
        50,
        "the oldest samples were evicted"
    );
    for w in all.windows(2) {
        assert!(
            w[0].snapshot.seq < w[1].snapshot.seq,
            "oldest-to-newest, a chart never sorts"
        );
    }

    ring.clear();
    assert!(ring.is_empty());
}
