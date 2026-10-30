use super::*;

pub(super) const NOT_FOUND: &str = "Game not found.";
pub(super) fn game_name(raw: &str) -> Result<String, V2Error> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(V2Error::invalid_argument("A game needs a name."));
    }
    if name.chars().count() > 128 {
        return Err(V2Error::invalid_argument(
            "A game name is at most 128 characters.",
        ));
    }
    Ok(name.to_owned())
}
pub(super) fn engine_type(raw: &str) -> Result<String, V2Error> {
    let engine = raw.trim().to_ascii_lowercase();
    if ENGINE_TYPES.contains(&engine.as_str()) {
        Ok(engine)
    } else {
        Err(V2Error::invalid_argument(format!(
            "Unknown engine type '{raw}'; expected one of {}.",
            ENGINE_TYPES.join(", ")
        )))
    }
}
pub fn backend_mode_for_engine(engine_type: &str) -> &'static str {
    match engine_type {
        "web" | "custom" => "wasm",
        _ => "relay",
    }
}
pub(super) fn game_status(raw: &str) -> Result<String, V2Error> {
    let status = raw.trim().to_ascii_lowercase();
    if GAME_STATUSES.contains(&status.as_str()) {
        Ok(status)
    } else {
        Err(V2Error::invalid_argument(format!(
            "Unknown game status '{raw}'; expected one of {}.",
            GAME_STATUSES.join(", ")
        )))
    }
}
pub(super) fn config_value(
    raw: Option<&serde_json::Value>,
) -> Result<Option<serde_json::Value>, V2Error> {
    match raw {
        None => Ok(None),
        Some(value @ serde_json::Value::Object(_)) => Ok(Some(value.clone())),
        Some(_) => Err(V2Error::invalid_argument(
            "A game config must be a JSON object.",
        )),
    }
}
pub(super) fn game_key(name: &str) -> String {
    let slug: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .take(50)
        .collect();
    let slug = slug.trim_matches('_');
    let slug = if slug.is_empty() { "game" } else { slug };
    let suffix = rand::thread_rng().gen_range(0..0x100_0000u32);
    format!("gm_{slug}_{suffix:06x}")
}
pub(super) fn api_key() -> String {
    credential(super::super::API_KEY_PREFIX)
}
pub(super) fn client_key() -> String {
    credential(super::super::CLIENT_KEY_PREFIX)
}
fn credential(prefix: &str) -> String {
    let mut bytes = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut bytes);
    let digest = Sha256::digest(bytes);
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("{prefix}{hex}")
}
pub(super) fn secrets_match(stored: &[u8], presented: &[u8]) -> bool {
    let mut diff = stored.len() ^ presented.len();
    for i in 0..stored.len().min(presented.len()) {
        diff |= (stored[i] ^ presented[i]) as usize;
    }
    diff == 0
}
pub(super) fn advertised_base_url(state: &ApiState, headers: &HeaderMap) -> String {
    let forwarded = headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    let scheme = if forwarded.trim().eq_ignore_ascii_case("https") {
        "https"
    } else {
        "http"
    };
    format!(
        "{scheme}://{}:{}",
        client_host(headers),
        state.control_port()
    )
}
fn stored_config(raw: Option<&serde_json::Value>) -> Result<serde_json::Value, V2Error> {
    match config_value(raw)? {
        Some(config) => Ok(config),
        None => serde_json::from_str(DEFAULT_GAME_CONFIG_JSON)
            .or_else(|_| serde_json::to_value(GameConfig::default()))
            .map_err(|_| V2Error::internal("The default game config is not valid JSON.")),
    }
}
pub(super) fn create_config(body: &CreateGameRequest) -> Result<serde_json::Value, V2Error> {
    let mut config = stored_config(body.config.as_ref())?;
    let map = config
        .as_object_mut()
        .expect("stored_config only returns objects");
    if let Some(tick_rate) = body.tick_rate {
        map.insert("tick_rate".to_owned(), json!(tick_rate));
    }
    if let Some(max_clients) = body.max_clients {
        map.insert("max_clients".to_owned(), json!(max_clients));
    }
    if let Some(persistent_rooms) = body.persistent_rooms {
        map.insert("persistent_rooms".to_owned(), json!(persistent_rooms));
    }
    if let Some(auto_close_seconds) = body.auto_close_seconds {
        map.insert("auto_close_seconds".to_owned(), json!(auto_close_seconds));
    }
    Ok(config)
}
