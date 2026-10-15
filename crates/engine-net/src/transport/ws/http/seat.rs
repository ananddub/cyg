//! Seat reservation the transport serves itself, and the JSON / JOIN_ROOM
//! shapes the SDK consumes.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use super::super::error::MatchMakeError;
use super::super::protocol::{MATCHMAKE_METHODS, PROTO_JOIN_ROOM, SERIALIZER_ID};

/// Default matchmake served when the game has not registered a hook: seats for
/// the full Colyseus method whitelist, reconnect via retained tokens, and
/// `MATCHMAKE_NO_HANDLER` for anything else.
pub(super) fn default_matchmake(
    method: &str,
    room: &str,
    body: &str,
    retained: &HashMap<String, String>,
    fallback_session: &str,
) -> Result<String, MatchMakeError> {
    if method == "reconnect" {
        let token = parse_json_string(body, "reconnectionToken").unwrap_or_default();
        if retained.contains_key(&token) {
            let session = token.split(':').nth(1).unwrap_or(fallback_session);
            return Ok(seat_json(room, session, &token));
        }
        return Err(MatchMakeError::expired(
            "reconnection token expired or unknown",
        ));
    }

    if !MATCHMAKE_METHODS.contains(&method) {
        return Err(MatchMakeError::no_handler(format!(
            "no room handler for method '{}'",
            method
        )));
    }

    let session = format!("sdk_{}", now_millis());
    let token = format!("{}:{}", room, session);
    Ok(seat_json(room, &session, &token))
}

/// `ISeatReservation` JSON consumed by the SDK (`packages/sdk/src/Client.ts`).
fn seat_json(room: &str, session_id: &str, token: &str) -> String {
    format!(
        "{{\"name\":\"{room}\",\"sessionId\":\"{sid}\",\"roomId\":\"{room}\",\"processId\":\"openoxide_1\",\"reconnectionToken\":\"{token}\"}}",
        room = room,
        sid = session_id,
        token = token
    )
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// Minimal JSON string-field scanner for seat-reservation bodies
/// (e.g. `{"reconnectionToken":"game_room:sess_1"}`).
pub(super) fn parse_json_string(body: &str, key: &str) -> Option<String> {
    let needle = format!("\"{}\"", key);
    let idx = body.find(&needle)?;
    let rest = body[idx + needle.len()..].strip_prefix(':')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Build the SDK-format JOIN_ROOM handshake frame:
/// [10][rt-len][rt][serializer-len]["schema"][msgpack 0 = empty state reflection]
pub(super) fn build_join_reply(
    session_id: &str,
    room_id: &str,
    seat_tokens: &HashMap<String, String>,
) -> Vec<u8> {
    let rt = seat_tokens
        .get(session_id)
        .cloned()
        .unwrap_or_else(|| format!("{}:{}", room_id, session_id));
    let mut reply = Vec::with_capacity(1 + 1 + rt.len() + 1 + SERIALIZER_ID.len() + 1);
    reply.push(PROTO_JOIN_ROOM);
    reply.push(rt.len() as u8);
    reply.extend_from_slice(rt.as_bytes());
    reply.push(SERIALIZER_ID.len() as u8);
    reply.extend_from_slice(SERIALIZER_ID);
    reply.push(0x00); // msgpack fixint 0: empty state reflection
    reply
}
