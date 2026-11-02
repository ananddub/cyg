
use ahash::AHashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use super::super::error::MatchMakeError;
use super::super::protocol::{MATCHMAKE_METHODS, PROTO_JOIN_ROOM, SERIALIZER_ID};

static SEAT_SEQ: AtomicU64 = AtomicU64::new(0);

pub(super) fn default_matchmake(
    method: &str,
    room: &str,
    body: &str,
    retained: &AHashMap<String, String>,
    fallback_session: &str,
    game_port: u16,
    udp_port: u16,
) -> Result<String, MatchMakeError> {
    if method == "reconnect" {
        let token = parse_json_string(body, "reconnectionToken").unwrap_or_default();
        if retained.contains_key(&token) {
            let session = token.split(':').nth(1).unwrap_or(fallback_session);
            return Ok(seat_json(room, session, &token, game_port, udp_port));
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

    let session = next_session_id();
    let token = format!("{}:{}", room, session);
    Ok(seat_json(room, &session, &token, game_port, udp_port))
}

fn next_session_id() -> String {
    let seq = SEAT_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("sdk_{}_{seq}", now_millis())
}

fn seat_json(room: &str, session_id: &str, token: &str, game_port: u16, udp_port: u16) -> String {
    format!(
        "{{\"name\":\"{room}\",\"sessionId\":\"{sid}\",\"roomId\":\"{room}\",\"processId\":\"cyg_1\",\"reconnectionToken\":\"{token}\",\"gamePort\":{game_port},\"udpPort\":{udp_port}}}",
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

pub(super) fn parse_json_string(body: &str, key: &str) -> Option<String> {
    let needle = format!("\"{}\"", key);
    let idx = body.find(&needle)?;
    let rest = body[idx + needle.len()..].strip_prefix(':')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

pub(super) fn build_join_reply(
    session_id: &str,
    room_id: &str,
    seat_tokens: &AHashMap<String, String>,
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
    reply.push(0x00);
    reply
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_ids_are_unique_within_a_millisecond() {
        let mut seen = std::collections::HashSet::new();
        for _ in 0..10_000 {
            let id = next_session_id();
            assert!(
                !id.contains(':'),
                "id must not break the room:session token"
            );
            assert!(seen.insert(id), "duplicate seat id minted");
        }
    }

    #[test]
    fn seat_advertises_the_bound_ports() {
        let body = default_matchmake(
            "joinOrCreate",
            "game_room",
            "{}",
            &AHashMap::new(),
            "fallback",
            9123,
            9124,
        )
        .expect("seat issued");
        assert!(body.contains("\"gamePort\":9123"), "{body}");
        assert!(body.contains("\"udpPort\":9124"), "{body}");
    }

    #[test]
    fn matchmake_issues_distinct_seats() {
        let retained = AHashMap::new();
        let mut seen = std::collections::HashSet::new();
        for _ in 0..1_000 {
            let body = default_matchmake(
                "joinOrCreate",
                "game_room",
                "{}",
                &retained,
                "fallback",
                7777,
                7778,
            )
            .expect("seat issued");
            let sid = parse_json_string(&body, "sessionId").expect("sessionId in body");
            assert!(
                seen.insert(sid),
                "duplicate sessionId across matchmake calls"
            );
        }
    }
}
