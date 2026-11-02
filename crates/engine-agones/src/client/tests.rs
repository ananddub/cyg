use std::time::Duration;

use super::{Reply, SidecarConfig, truncate};

fn reply_bytes(body: &[u8]) -> Reply {
    Reply {
        status: 200,
        reason: "OK".into(),
        body: body.to_vec(),
    }
}

#[test]
fn reply_success_and_lossy_body() {
    assert!(reply_bytes(b"").is_success());
    let r = Reply {
        status: 400,
        reason: "Bad Request".into(),
        body: vec![0xff, b'x'],
    };
    assert!(!r.is_success());
    assert!(r.body_lossy().contains('x'));
}

#[test]
fn truncation_never_splits_a_char() {
    assert_eq!(truncate("héllo", 1), "h…");
    assert_eq!(truncate("short", 99), "short");
}

#[test]
fn config_builds_host_port_addr() {
    let cfg = SidecarConfig::new("127.0.0.1", 9358, Duration::from_millis(500));
    assert_eq!(cfg.addr, "127.0.0.1:9358");
    assert_eq!(
        cfg.clone().with_timeout(Duration::from_secs(2)).timeout,
        Duration::from_secs(2)
    );
    assert_eq!(SidecarConfig::loopback(9358).addr, "127.0.0.1:9358");
}
