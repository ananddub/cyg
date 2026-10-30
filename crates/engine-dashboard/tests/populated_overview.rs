use engine_dashboard::{
    Capacity, Overview, ServerInfo, TICK_HISTOGRAM_BUCKETS, TickHealth, TransportMix, histogram_add,
};

#[test]
fn a_populated_overview_serialises_and_keeps_passthrough_camel_case() {
    let mut tick = TickHealth::default();
    let mut hist = [0u32; TICK_HISTOGRAM_BUCKETS];
    histogram_add(&mut hist, 900);
    tick.histogram = hist;
    let overview = Overview {
        server: ServerInfo {
            name: "tank-3d".into(),
            port: 7777,
            control_port: 8124,
            web_port: 7779,
        },
        ccu: 3,
        rooms: 1,
        capacity: Capacity { players: 3, max: 8 },
        tick,
        transport: TransportMix {
            websocket: 2,
            webrtc: 1,
            native_udp: 0,
            signal_peers: 4,
            ..Default::default()
        },
        agones: serde_json::json!({ "enabled": false, "consecutiveFailures": 0 }),
        has_sim_sample: true,
        ..Default::default()
    };
    let json = serde_json::to_value(overview).unwrap();
    let mut queue = vec![serde_json::Value::Object(json.as_object().unwrap().clone())];
    while let Some(value) = queue.pop() {
        match value {
            serde_json::Value::Object(map) => {
                for (key, child) in map {
                    assert!(!key.contains('_'), "snake_case key `{key}` in wire payload");
                    queue.push(child);
                }
            }
            serde_json::Value::Array(items) => queue.extend(items),
            _ => {}
        }
    }
    assert_eq!(json["agones"]["consecutiveFailures"].as_u64(), Some(0));
}
