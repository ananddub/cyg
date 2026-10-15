/// Tell the sidecar to retire this pod and stop the beat loop.
///
/// Idempotent: `Ok(true)` means this call sent `POST /shutdown`, `Ok(false)`
/// means it was already sent. A pod whose sidecar is gone is a pod that is
/// already going away, so the stop path should log the error and move on.
pub fn shutdown() -> Result<bool, String> {
    engine_agones::shutdown_sidecar()
}

/// Stop heartbeating without asserting a shutdown transition.
#[allow(dead_code)]
pub fn stop_heartbeats() -> bool {
    engine_agones::stop_health_loop()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutdown_without_a_lifecycle_is_a_told_reason_not_a_panic() {
        // Either nothing is installed (the usual test case) or a lifecycle from
        // another test is: both must return without touching a socket we do not own.
        let result = shutdown();
        assert!(result.is_ok() || result.unwrap_err().contains("no Agones lifecycle"));
    }
}
