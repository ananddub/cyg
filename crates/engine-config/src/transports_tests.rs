use crate::transports::{Lane, TransportsConfig};

#[test]
fn defaults_open_browser_and_engine_lanes_only() {
    let t = TransportsConfig::default();
    assert!(t.enabled(Lane::WebRtc));
    assert!(t.enabled(Lane::WebSocket));
    assert!(
        !t.enabled(Lane::Udp),
        "native udp collided by default before"
    );
    assert!(!t.enabled(Lane::Quic));
}

#[test]
fn a_lane_can_be_turned_off_including_webrtc() {
    let mut t = TransportsConfig::default();
    t.set_enabled(Lane::WebRtc, false);
    assert!(!t.enabled(Lane::WebRtc));
    assert_eq!(t.active(), vec![Lane::WebSocket]);
}

#[test]
fn turning_every_lane_off_is_refused() {
    let mut t = TransportsConfig::default();
    for lane in Lane::ALL {
        t.set_enabled(lane, false);
    }
    let err = t.validate().expect_err("no lanes must be refused");
    assert!(err.contains("no transports enabled"), "{err}");
}

#[test]
fn lane_names_round_trip_through_parse() {
    for lane in Lane::ALL {
        assert_eq!(Lane::parse(lane.as_str()), Some(lane));
    }
    assert_eq!(Lane::parse("ws"), Some(Lane::WebSocket));
    assert_eq!(Lane::parse("webscoket"), None);
}

#[test]
fn summary_lists_what_runs_and_what_does_not() {
    let t = TransportsConfig::default();
    let summary = t.summary();
    assert!(summary.contains("webrtc"), "{summary}");
    assert!(summary.contains("websocket"), "{summary}");
    assert!(summary.contains("udp, quic off"), "{summary}");
}

#[test]
#[should_panic(expected = "was removed")]
fn a_removed_port_key_is_refused_not_ignored() {
    // Silently ignoring `quic_port` would leave the lane at its default and the
    // operator believing they configured it.
    crate::parse_config("name = \"x\"\nquic_port = 7778\n");
}

#[test]
#[should_panic(expected = "was removed")]
fn the_removed_ws_port_key_is_refused_too() {
    crate::parse_config("name = \"x\"\nws_port = 7796\n");
}

#[test]
fn the_transports_table_switches_lanes() {
    let cfg = crate::parse_config(
        "name = \"x\"\n[transports]\nwebrtc = false\nudp = true\nwebsocket = false\n",
    );
    assert!(!cfg.transports.enabled(Lane::WebRtc));
    assert!(!cfg.transports.enabled(Lane::WebSocket));
    assert!(cfg.transports.enabled(Lane::Udp));
    assert!(
        !cfg.transports.enabled(Lane::Quic),
        "unset lanes keep their default"
    );
}
