use super::*;

#[test]
fn an_idle_overview_carries_every_key_the_dashboard_reads() {
    let json = serde_json::to_value(Overview::default()).unwrap();
    for path in [
        "/server/name",
        "/server/port",
        "/server/controlPort",
        "/server/webPort",
        "/uptimeSec",
        "/ccu",
        "/rooms",
        "/capacity/players",
        "/capacity/max",
        "/tick/hz",
        "/tick/configuredHz",
        "/tick/nominalUs",
        "/tick/p50Us",
        "/tick/p99Us",
        "/tick/overruns",
        "/tick/samples",
        "/tick/seq",
        "/tick/histogram",
        "/transport/websocket",
        "/transport/webrtc",
        "/transport/nativeUdp",
        "/transport/signalPeers",
        "/transport/signalsDelivered",
        "/transport/signalsDropped",
        "/bus/inputsQueued",
        "/bus/commandsQueued",
        "/bus/deltasQueued",
        "/bus/deltasSent",
        "/bus/deltasDroppedFull",
        "/bus/inputsDropped",
        "/seats/outstanding",
        "/seats/minted",
        "/seats/rejected",
        "/seats/refused",
        "/seats/bound",
        "/seats/ttlSecs",
        "/rateLimit/allowed",
        "/rateLimit/denied",
        "/rateLimit/squeezed",
        "/rateLimit/swept",
        "/rateLimit/tracked",
        "/rateLimit/rps",
        "/rateLimit/burst",
        "/agones",
        "/hasSimSample",
        "/sampleAgeMs",
    ] {
        assert!(json.pointer(path).is_some(), "missing {path} in {json}");
    }
    assert_eq!(
        json["tick"]["histogram"].as_array().unwrap().len(),
        crate::TICK_HISTOGRAM_BUCKETS
    );
    assert_eq!(json["hasSimSample"].as_bool(), Some(false));
}
