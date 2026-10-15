//! Signaling surface helpers.

/// Malformed ICE text becomes an empty list rather than a failed connection:
/// the config is operator-supplied, and LAN-only still works.
pub fn parse_ice(config: &str) -> serde_json::Value {
    serde_json::from_str(config).unwrap_or_else(|_| serde_json::Value::Array(vec![]))
}
