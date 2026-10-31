pub fn parse_ice(config: &str) -> serde_json::Value {
    serde_json::from_str(config).unwrap_or_else(|_| serde_json::Value::Array(vec![]))
}
