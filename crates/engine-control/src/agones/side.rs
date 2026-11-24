pub fn shutdown() -> Result<bool, String> {
    engine_agones::shutdown_sidecar()
}

#[allow(dead_code)]
pub fn stop_heartbeats() -> bool {
    engine_agones::stop_health_loop()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutdown_without_a_lifecycle_is_a_told_reason_not_a_panic() {
        let result = shutdown();
        assert!(result.is_ok() || result.unwrap_err().contains("no Agones lifecycle"));
    }
}
