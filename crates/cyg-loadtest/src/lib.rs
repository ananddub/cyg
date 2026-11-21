//! Shared protocol glue for the cyg-loadtest CLI.
//!
//! Everything that touches the wire goes through `cyg-core-rs` — this module
//! only classifies raw transport frames and builds the two outbound frames the
//! load tool needs (the schema-input app message and the full-snapshot ack).

use cyg_core_rs::input::InputValue;
use cyg_core_rs::state::{KIND_FULL, KIND_PATCH, KIND_WORLD_FULL, KIND_WORLD_PATCH};

/// cyg.js `AppOp.MESSAGE` — typed app-message envelope on the WS lane.
pub const APP_OP_MESSAGE: u8 = 0x05;
/// Reserved type the host's input lane dispatches on (`room.send('__input', body)`).
pub const INPUT_TYPE: &str = "__input";
/// cyg.js `_ackSchema()`: the client must ack every full keyframe (kind 5 or
/// 7) or the host keeps re-sending the full snapshot instead of field patches.
pub const SCHEMA_READY_TYPE: &str = "__ox_schema_ready";
/// Descriptor magic pushed by the host right after JOIN_ROOM.
pub const DESCRIPTOR_MAGIC: &[u8; 4] = b"OXSD";
const SCHEMA_MAGIC: &[u8; 4] = b"OXS2";

/// One inbound binary WS frame, classified the same way cyg.js
/// `_handleInboundBinary` does it.
#[derive(Debug, PartialEq)]
pub enum Inbound {
    /// "OXSD" descriptor frame — bytes are the full descriptor (parse via
    /// `cyg_core_rs::ClientCore::new`).
    Descriptor,
    /// Schema delta carrying the `[0xA5 "delta"]` envelope — feed straight
    /// into `ClientCore::apply_payload` (it strips prefix + envelope itself).
    Delta,
    /// Anything else (pings, app messages, voice/signaling) — ignored.
    Other,
}

pub fn classify_frame(frame: &[u8]) -> Inbound {
    if frame.len() >= 8 && &frame[..4] == DESCRIPTOR_MAGIC {
        return Inbound::Descriptor;
    }
    // Delta: bare [0xA5 "delta"], [0x11][0xA5 "delta"], or a reassembled body
    // that already lost its prefix and starts on the envelope ([id]["OXS2"]).
    if frame.len() >= 22 {
        let prefixed = frame[0] == 0xA5
            || (frame[0] == 0x11 && frame.get(1) == Some(&0xA5));
        let body_at = if frame[0] == 0x11 && frame.get(1) == Some(&0xA5) {
            7 // [0x11][0xA5 "delta"]
        } else if frame[0] == 0xA5 {
            6 // [0xA5 "delta"]
        } else {
            0 // prefix-less (fragment reassembly path)
        };
        if (prefixed || body_at == 0)
            && frame.len() > body_at + 8 + 4
            && &frame[body_at + 8..body_at + 12] == SCHEMA_MAGIC
        {
            return Inbound::Delta;
        }
    }
    Inbound::Other
}

/// OXS2 frame kind byte (5 full collection, 6 patch, 7/8 packed world
/// full/patch) of a classified delta frame, or None when it cannot be read.
pub fn delta_kind(frame: &[u8]) -> Option<u8> {
    let at = if frame[0] == 0x11 && frame.get(1) == Some(&0xA5) {
        7 + 8 + 4
    } else if frame[0] == 0xA5 {
        6 + 8 + 4
    } else {
        8 + 4
    };
    frame.get(at).copied()
}

pub fn is_full_kind(kind: u8) -> bool {
    kind == KIND_FULL || kind == KIND_WORLD_FULL
}

pub fn is_patch_kind(kind: u8) -> bool {
    kind == KIND_PATCH || kind == KIND_WORLD_PATCH
}

/// Typed app-message envelope (cyg.js `_encodeAppMessage`):
/// `[0x05][type_len][type UTF-8][body]`.
pub fn encode_app_message(type_name: &str, body: &[u8]) -> Vec<u8> {
    let type_bytes = type_name.as_bytes();
    assert!(!type_bytes.is_empty() && type_bytes.len() <= 255);
    let mut frame = Vec::with_capacity(2 + type_bytes.len() + body.len());
    frame.push(APP_OP_MESSAGE);
    frame.push(type_bytes.len() as u8);
    frame.extend_from_slice(type_bytes);
    frame.extend_from_slice(body);
    frame
}

/// Encode one schema input through the shared core and wrap it in the
/// `__input` app-message envelope.
pub fn encode_schema_input_with(
    core: &cyg_core_rs::ClientCore,
    seq: u16,
    values: &[InputValue],
) -> Result<Vec<u8>, String> {
    let body = core.encode_schema_input(seq, values)?;
    Ok(encode_app_message(INPUT_TYPE, &body))
}

/// Full-snapshot ack (cyg.js `_ackSchema`): an empty `__ox_schema_ready`
/// app message.
pub fn encode_schema_ready_ack() -> Vec<u8> {
    encode_app_message(SCHEMA_READY_TYPE, &[])
}

/// Build the Input values for one tick, mirroring loadtest.mjs `--move`
/// semantics on the schema lane: numeric movement fields alternate +1/-1 each
/// second, bools stay false, other kinds zero. Field names match the room's
/// declared `Input` schema (order preserved — the core packs in order).
pub fn movement_values(
    fields: &[cyg_core_rs::SchemaField],
    elapsed_ms: u64,
) -> Result<Vec<InputValue>, String> {
    let dir = if (elapsed_ms / 1000) % 2 == 0 { 1.0 } else { -1.0 };
    fields
        .iter()
        .map(|f| match f.kind {
            cyg_core_rs::WireKind::F32 => Ok(InputValue::F32(dir as f32)),
            cyg_core_rs::WireKind::F64 => Ok(InputValue::F64(dir)),
            cyg_core_rs::WireKind::Bool => Ok(InputValue::Bool(false)),
            cyg_core_rs::WireKind::U8 => Ok(InputValue::U8(0)),
            cyg_core_rs::WireKind::U16 => Ok(InputValue::U16(0)),
            cyg_core_rs::WireKind::I16 => Ok(InputValue::I16(0)),
            cyg_core_rs::WireKind::U32 => Ok(InputValue::U32(0)),
            cyg_core_rs::WireKind::I32 => Ok(InputValue::I32(0)),
            cyg_core_rs::WireKind::I64 => Ok(InputValue::I64(0)),
            cyg_core_rs::WireKind::I8 => Ok(InputValue::I8(0)),
            other => Err(format!(
                "Input schema field '{}' has unsupported kind {:?} — movement values can only be numeric/bool",
                f.name, other
            )),
        })
        .collect()
}

pub fn percentile(values: &[f64], p: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let idx = ((p / 100.0) * sorted.len() as f64).ceil() as usize;
    sorted[idx.saturating_sub(1).min(sorted.len() - 1)]
}

pub fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

/// Matchmake seat fields the CLI consumes (subset of the real reply).
#[derive(Debug, Clone)]
pub struct Seat {
    pub name: String,
    pub session_id: String,
    pub game_port: u16,
}

pub fn parse_seat(json: &serde_json::Value) -> Result<Seat, String> {
    let session_id = json
        .get("sessionId")
        .and_then(|v| v.as_str())
        .ok_or("matchmake reply carried no sessionId")?
        .to_string();
    let name = json
        .get("name")
        .and_then(|v| v.as_str())
        .or_else(|| json.get("roomId").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();
    let game_port = json.get("gamePort").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
    Ok(Seat { name, session_id, game_port })
}

/// cyg.js `_connectWebSocket`: `ws://host:{seat.gamePort}/{room}?sessionId=…`
pub fn build_ws_url(http_base: &str, seat: &Seat, room_fallback: &str) -> String {
    let base = http_base
        .trim_end_matches('/')
        .replacen("https://", "wss://", 1)
        .replacen("http://", "ws://", 1);
    // strip any path prefix — the room-data lane lives at the root of the host
    let authority = base
        .trim_start_matches("wss://")
        .trim_start_matches("ws://")
        .split('/')
        .next()
        .unwrap_or("")
        .to_string();
    let (host, default_port) = match authority.rsplit_once(':') {
        Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) => (h.to_string(), p.to_string()),
        _ => (authority.clone(), if base.starts_with("wss://") { "443".into() } else { "80".into() }),
    };
    let port = if seat.game_port > 0 {
        seat.game_port.to_string()
    } else {
        default_port
    };
    let room = if seat.name.is_empty() { room_fallback } else { &seat.name };
    format!("ws://{host}:{port}/{room}?sessionId={}", seat.session_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_descriptor_delta_and_other() {
        let mut descriptor = vec![0x4F, 0x58, 0x53, 0x44, 3, 0, 0];
        descriptor.extend_from_slice(&[0u8; 64]);
        assert_eq!(classify_frame(&descriptor), Inbound::Descriptor);

        let mut delta = vec![0xA5u8];
        delta.extend_from_slice(b"delta");
        delta.extend_from_slice(&1i64.to_le_bytes());
        delta.extend_from_slice(b"OXS2");
        delta.push(1); // kind
        delta.extend_from_slice(&[0u8; 16]);
        assert_eq!(classify_frame(&delta), Inbound::Delta);
        assert_eq!(delta_kind(&delta), Some(1));

        let mut prefixed = vec![0x11u8, 0xA5];
        prefixed.extend_from_slice(b"delta");
        prefixed.extend_from_slice(&1i64.to_le_bytes());
        prefixed.extend_from_slice(b"OXS2");
        prefixed.push(4);
        prefixed.extend_from_slice(&[0u8; 16]);
        assert_eq!(classify_frame(&prefixed), Inbound::Delta);
        assert_eq!(delta_kind(&prefixed), Some(4));
        assert!(is_patch_kind(4) && !is_full_kind(4));

        assert_eq!(classify_frame(&[18, 1, 2, 3, 4]), Inbound::Other);
        assert_eq!(classify_frame(&[0x05, 4, b'p', b'i', b'n', b'g']), Inbound::Other);
    }

    #[test]
    fn builds_ws_url_like_cygjs() {
        let seat = Seat {
            name: "arena".into(),
            session_id: "1794789956-0".into(),
            game_port: 7796,
        };
        assert_eq!(
            build_ws_url("http://127.0.0.1:8297", &seat, "arena"),
            "ws://127.0.0.1:7796/arena?sessionId=1794789956-0"
        );
        let no_port = Seat { name: String::new(), session_id: "s".into(), game_port: 0 };
        assert_eq!(
            build_ws_url("http://127.0.0.1:8297", &no_port, "bench"),
            "ws://127.0.0.1:8297/bench?sessionId=s"
        );
    }

    #[test]
    fn app_message_envelope_matches_cygjs() {
        let frame = encode_app_message("__input", &[0x01, 0x02]);
        assert_eq!(frame[0], 0x05);
        assert_eq!(frame[1], 7);
        assert_eq!(&frame[2..9], b"__input");
        assert_eq!(&frame[9..], &[0x01, 0x02]);
        assert_eq!(encode_schema_ready_ack(), encode_app_message("__ox_schema_ready", &[]));
    }

    #[test]
    fn movement_matches_loadtest_move_pattern() {
        let fields = vec![
            cyg_core_rs::SchemaField {
                name: "moveX".into(),
                kind: cyg_core_rs::WireKind::F32,
                element: None,
                ref_target: None,
                flags: 0,
                source_type: String::new(),
                default: None,
                line: 0,
                column: 0,
            },
            cyg_core_rs::SchemaField {
                name: "jump".into(),
                kind: cyg_core_rs::WireKind::Bool,
                element: None,
                ref_target: None,
                flags: 0,
                source_type: String::new(),
                default: None,
                line: 0,
                column: 0,
            },
        ];
        let vals = movement_values(&fields, 500).unwrap();
        assert_eq!(vals[0], InputValue::F32(1.0));
        assert_eq!(vals[1], InputValue::Bool(false));
        let vals = movement_values(&fields, 1500).unwrap();
        assert_eq!(vals[0], InputValue::F32(-1.0));
    }

    #[test]
    fn seat_parse_and_percentiles() {
        let seat = parse_seat(&serde_json::json!({
            "name": "arena", "sessionId": "x-1", "gamePort": 7796
        }))
        .unwrap();
        assert_eq!(seat.game_port, 7796);
        assert_eq!(percentile(&[1.0, 2.0, 3.0, 4.0], 50.0), 2.0);
        assert_eq!(percentile(&[1.0, 2.0, 3.0, 4.0], 95.0), 4.0);
        assert_eq!(mean(&[2.0, 4.0]), 3.0);
    }
}
