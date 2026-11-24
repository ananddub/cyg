use ahash::AHashMap;
use engine_config::RoomVariantDef;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
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
    variants: &std::collections::BTreeMap<String, RoomVariantDef>,
    clients_meta: &AHashMap<std::sync::Arc<str>, super::super::hooks::ClientMeta>,
) -> Result<String, MatchMakeError> {
    let chain = chain_tracker();
    let Ok(mut chain) = chain.lock() else {
        // tracker poisoned — degrade to no chaining rather than refuse joins
        let session = next_session_id();
        let token = format!("{}:{}", room, session);
        return Ok(seat_json(room, &session, &token, game_port, udp_port));
    };
    let now = Instant::now();
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

    // Variant matchmaking: the variant's filter_by keys split the join
    // options into a deterministic room key, mirroring the control plane.
    // When that room is at the variant's cap, the join CHAINS into the next
    // generation (`...__g1`, `__g2`…) instead of being refused. Occupancy is
    // counted from this lane's own connected clients, so it is live.
    let room_key = variants
        .get(room)
        .map(|variant| format!("{room}{}", variant.room_key_suffix(body)))
        .unwrap_or_else(|| room.to_string());

    let room_key = if let Some(variant) = variants.get(room) {
        let cap = variant.max_clients.unwrap_or(0);
        if cap > 0 {
            let base = room_key;
            let client_count = |key: &str| -> u32 {
                clients_meta
                    .values()
                    .filter(|meta| meta.room.as_deref() == Some(key))
                    .count() as u32
            };
            let mut generation = 0u32;
            loop {
                let key = if generation == 0 {
                    base.clone()
                } else {
                    format!("{base}__g{generation}")
                };
                if (client_count(&key) as i64 + chain.outstanding(&key, now) as i64) < cap as i64 {
                    chain.record(&key, now);
                    break key;
                }
                generation += 1;
                if generation > 512 {
                    break format!("{base}__g{generation}");
                }
            }
        } else {
            room_key
        }
    } else {
        room_key
    };

    let session = next_session_id();
    let token = format!("{}:{}", room_key, session);
    Ok(seat_json(&room_key, &session, &token, game_port, udp_port))
}

/// Seats minted recently per room key — the join burst inside one occupancy
/// refresh still counts toward the cap. Entries age out and stop reserving.
#[derive(Default)]
struct ChainState {
    outstanding: HashMap<String, Vec<Instant>>,
}

impl ChainState {
    fn record(&mut self, room_key: &str, now: Instant) {
        self.outstanding
            .entry(room_key.to_string())
            .or_default()
            .push(now);
    }

    fn outstanding(&mut self, room_key: &str, now: Instant) -> usize {
        let Some(list) = self.outstanding.get_mut(room_key) else {
            return 0;
        };
        const TTL: Duration = Duration::from_secs(15);
        list.retain(|at| now.duration_since(*at) < TTL);
        list.len()
    }
}

fn chain_tracker() -> &'static Mutex<ChainState> {
    static TRACKER: OnceLock<Mutex<ChainState>> = OnceLock::new();
    TRACKER.get_or_init(|| Mutex::new(ChainState::default()))
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
    seat_token: Option<&str>,
) -> Vec<u8> {
    let rt = seat_token
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{room_id}:{session_id}"));
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
            &std::collections::BTreeMap::new(),
            &AHashMap::new(),
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
                &std::collections::BTreeMap::new(),
                &AHashMap::new(),
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
