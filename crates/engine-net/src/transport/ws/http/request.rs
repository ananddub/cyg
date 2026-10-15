//! The greeting's request line and headers, parsed into owned values up front
//! so the borrow of the receive buffer ends before the client is mutated.

/// One parsed HTTP greeting: matchmake POST, static GET/HEAD or WS upgrade.
pub(super) struct Greeting {
    pub(super) is_options: bool,
    pub(super) is_matchmake: bool,
    /// Room name from `/matchmake/<method>/<room>`.
    pub(super) room_name: String,
    /// Request `Origin`, or `*` when the client sent none.
    pub(super) origin: String,
    /// `sessionId` query parameter — the SDK's seat-reservation id.
    pub(super) url_session_id: Option<String>,
    /// URL path segments after the leading `/`: matchmake is
    /// `[matchmake, method, room]`, a WS upgrade is `[processId, roomId]`.
    pub(super) url_method: String,
    pub(super) url_room: String,
    pub(super) url_body: String,
    pub(super) is_head: bool,
    pub(super) is_plain_get: bool,
    /// Path of a plain GET/HEAD, query string removed.
    pub(super) get_path: String,
}

pub(super) fn parse_greeting(raw: &[u8]) -> Greeting {
    let req = String::from_utf8_lossy(raw);
    let is_options = req.starts_with("OPTIONS");
    let is_matchmake = req.contains("/matchmake/") && !req.contains("Sec-WebSocket-Key");
    let room_name = if is_matchmake {
        req.split("/matchmake/")
            .nth(1)
            .and_then(|rest| rest.split('/').nth(1))
            .and_then(|name| name.split_whitespace().next())
            .unwrap_or("game_room")
            .to_string()
    } else {
        String::new()
    };
    let origin = req
        .lines()
        .find(|l| l.to_ascii_lowercase().starts_with("origin:"))
        .and_then(|l| l.split_once(':').map(|(_, v)| v.trim().to_string()))
        .unwrap_or_else(|| "*".to_string());
    let url_session_id = {
        let path = req.split_whitespace().nth(1).unwrap_or("");
        path.split('?')
            .nth(1)
            .and_then(|query| {
                query
                    .split('&')
                    .find_map(|kv| kv.strip_prefix("sessionId="))
            })
            .map(str::to_string)
    };
    let url_method = req
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|p| p.split('/').nth(2).map(str::to_string))
        .unwrap_or_default();
    let url_room = {
        let path = req.split_whitespace().nth(1).unwrap_or("");
        let query = path
            .split_once('?')
            .map(|(_, q)| format!("?{}", q))
            .unwrap_or_default();
        let clean = path.split('?').next().unwrap_or("").trim_matches('/');
        let segments: Vec<&str> = clean.split('/').filter(|s| !s.is_empty()).collect();
        let base_room = if segments.is_empty() {
            ""
        } else if segments.len() == 1 {
            segments[0]
        } else {
            segments.last().unwrap_or(&"")
        };
        if base_room.is_empty() {
            String::new()
        } else {
            format!("{}{}", base_room, query)
        }
    };
    let url_body = req.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
    let is_head = req.starts_with("HEAD ");
    let is_plain_get = (req.starts_with("GET ") || is_head) && !req.contains("Sec-WebSocket-Key");
    let get_path = if is_plain_get {
        req.split_whitespace()
            .nth(1)
            .unwrap_or("/")
            .split('?')
            .next()
            .unwrap_or("/")
            .to_string()
    } else {
        String::new()
    };

    Greeting {
        is_options,
        is_matchmake,
        room_name,
        origin,
        url_session_id,
        url_method,
        url_room,
        url_body,
        is_head,
        is_plain_get,
        get_path,
    }
}
