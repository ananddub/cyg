use super::*;
use crate::services::status::snapshot::status;

/// Every key a dashboard reads must be present even when nothing has happened yet — a
/// missing key is an `undefined` in the browser, indistinguishable from a crash.
#[tokio::test]
async fn status_reports_every_documented_group_on_an_idle_plane() {
    let body = json_of(status(&state()).await).await;

    for path in [
        "/plane/uptime_ms",
        "/plane/controlPort",
        "/plane/workers",
        "/plane/game/name",
        "/plane/game/port",
        "/plane/game/webPort",
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
        "/signaling/peers",
        "/signaling/delivered",
        "/signaling/droppedFull",
        "/agones/enabled",
        "/agones/state",
        "/agones/ready",
        "/agones/intervalMs",
        "/agones/uptimeMs",
        // `PingStats` is `#[serde(flatten)]`, so its counters sit directly
        // on the `agones` object rather than under a nested group.
        "/agones/successes",
        "/agones/consecutiveFailures",
        "/bus/inputsQueued",
        "/bus/commandsQueued",
        "/bus/deltasQueued",
    ] {
        assert!(body.pointer(path).is_some(), "missing {path} in {body}");
    }
    assert_eq!(body["plane"]["controlPort"].as_u64(), Some(8124));
}

/// A process that never configured Agones must say so: the group is present, `enabled` is
/// false and `state` is `Unknown`, not a plausible `Ready`.
#[tokio::test]
async fn agones_reports_absent_not_failing_when_no_sidecar_is_configured() {
    let body = json_of(status(&state()).await).await;
    assert_eq!(body["agones"]["enabled"].as_bool(), Some(false));
    assert_eq!(body["agones"]["ready"].as_bool(), Some(false));
    assert_eq!(
        body["agones"]["pings"].as_u64(),
        Some(0),
        "no heartbeats sent"
    );
}

/// A plane with no sim thread must say so. `updated_ms` is monotonic process time, so a
/// zeroed snapshot would look like a room that ticked once and then went quiet forever.
#[tokio::test]
async fn a_missing_sim_sample_is_null_never_zeros() {
    let body = json_of(status(&state()).await).await;
    assert!(
        body["sim"].is_null(),
        "sim must be null, got {}",
        body["sim"]
    );
}
