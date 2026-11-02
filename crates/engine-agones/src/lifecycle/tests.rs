use std::time::Duration;

use super::{AgonesSnapshot, Lifecycle};
use crate::client::SidecarConfig;

#[test]
fn disabled_snapshot_says_so() {
    let json = serde_json::to_string(&AgonesSnapshot::disabled()).unwrap();
    assert!(json.contains("\"enabled\":false"), "{json}");
    assert!(json.contains("\"state\":\"Unknown\""), "{json}");
    assert!(json.contains("\"consecutiveFailures\":0"), "{json}");
}

#[test]
fn snapshot_flattens_ping_stats() {
    let snap =
        Lifecycle::new(SidecarConfig::loopback(9358), Duration::from_millis(2000)).snapshot();
    let json = serde_json::to_string(&snap).unwrap();
    assert!(json.contains("\"addr\":\"127.0.0.1:9358\""), "{json}");
    assert!(json.contains("\"enabled\":true"), "{json}");
    assert!(json.contains("\"intervalMs\":2000"), "{json}");

    assert!(json.contains("\"pings\":0"), "{json}");
    assert!(!json.contains("\"pings\":{"), "{json}");
}
