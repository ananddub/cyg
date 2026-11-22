use engine_net::framing::OpCode;
use engine_net::transport::GameTransport;

use crate::rooms::{GameRoom, close_code};

use crate::host::*;
pub(crate) fn claimed_user_id(options: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(options).ok()?;
    value
        .get("userId")?
        .as_str()
        .map(str::to_string)
        .filter(|id| !id.is_empty())
}

pub(crate) fn flush_room_outbox(
    room: &mut GameRoom,
    transport: &mut impl GameTransport,
    room_name: &str,
    history: &EventHistory,
) {
    let tick = room.current_tick;
    let intents = room.drain_stat_intents();
    if !intents.is_empty() {
        history.record_stats(room_name, tick, intents);
    }

    let messages = room.drain_outbox();
    if messages.is_empty() {
        return;
    }

    let mut frame = Vec::with_capacity(64);
    // Reverse id→session index, built lazily once per flush (only when
    // targeted messages or kicks exist) instead of an O(sessions) linear
    // scan per targeted message.
    let mut id_to_session: std::collections::HashMap<i64, Option<std::rc::Rc<str>>> =
        HashMap::new();
    for message in messages {
        if message.event.is_empty() || message.event.len() > 255 {
            continue;
        }
        history.mirror(room_name, tick, &message);
        if history.carries_result(&message) {
            room.result = Some(message.payload.clone());
        }
        frame.clear();
        frame.push(APP_MESSAGE_OPCODE);
        frame.push(message.event.len() as u8);
        frame.extend_from_slice(message.event.as_bytes());
        frame.extend_from_slice(&message.payload);

        if message.target > 0 {
            let session = id_to_session
                .entry(message.target)
                .or_insert_with(|| {
                    room.sessions.iter().find_map(|(session, id)| {
                        (*id == message.target).then(|| std::rc::Rc::clone(session))
                    })
                })
                .clone();
            if let Some(session) = session {
                let _ = transport.send_to(&session, OpCode::RoomDataBytes, &frame);
            }
        } else if message.target < 0 {
            // Broadcast-except: the SDK's `broadcast(type, payload, except)`
            // packs `target = -playerId`; everyone else still receives it.
            let except = -message.target;
            for (session, &id) in &room.sessions {
                if id != except {
                    let _ = transport.send_to(session, OpCode::RoomDataBytes, &frame);
                }
            }
            for spectator in &room.spectators {
                let _ = transport.send_to(spectator, OpCode::RoomDataBytes, &frame);
            }
        } else if message.target == 0 {
            for session in room.sessions.keys() {
                let _ = transport.send_to(session, OpCode::RoomDataBytes, &frame);
            }
            for spectator in &room.spectators {
                let _ = transport.send_to(spectator, OpCode::RoomDataBytes, &frame);
            }
        }
    }

    // Client.leave() calls the guest queued during the last invocation —
    // close their connections now that the guest call has returned.
    for id in room.drain_kicks() {
        let session = id_to_session
            .entry(id)
            .or_insert_with(|| {
                room.sessions
                    .iter()
                    .find_map(|(session, &pid)| (pid == id).then(|| std::rc::Rc::clone(session)))
            })
            .clone();
        let Some(session) = session else {
            continue;
        };
        let _ = transport.disconnect_with(&session, close_code::KICKED as u16, "kicked by room");
        room.kick_player(&session, id);
    }
}

pub(crate) fn announce_voice_peer_change(
    transport: &mut impl GameTransport,
    mesh: &mut engine_voice::VoiceMesh,
    kind: engine_voice::SignalKind,
    subject: u32,
) {
    let frame = engine_voice::SignalFrame::new(
        kind,
        engine_voice::SERVER_SENDER,
        engine_voice::BROADCAST,
        bytes::Bytes::from(format!("[{subject}]")),
    )
    .encode();
    mesh.for_each_peer(subject, |target| {
        let _ = transport.send_to(target.session_id, OpCode::RoomDataBytes, &frame);
    });
}

pub(crate) fn reject_signal(
    transport: &mut impl GameTransport,
    client_id: &str,
    from: u32,
    reason: &'static str,
) {
    let frame = engine_voice::SignalFrame::new(
        engine_voice::SignalKind::Error,
        engine_voice::SERVER_SENDER,
        from,
        bytes::Bytes::from(format!(r#"{{"reason":"{reason}"}}"#)),
    )
    .encode();
    let _ = transport.send_to(client_id, OpCode::RoomDataBytes, &frame);
}

pub(crate) fn reject_room(transport: &mut LoopTransport, client_id: &str, message: &str) {
    let code = engine_net::close_code::WITH_ERROR;
    let reason = format!("{{\"code\":{code},\"message\":\"{message}\"}}");
    let _ = transport.send_to(client_id, OpCode::Error, reason.as_bytes());
    let _ = transport.disconnect_with(client_id, code, message);
    let _ = transport.flush();
}

pub(crate) fn close_room_sessions(
    transport: &mut LoopTransport,
    room: &GameRoom,
    code: u16,
    message: &str,
) {
    for session in room.sessions.keys().chain(room.spectators.iter()) {
        let _ = transport.disconnect_with(session, code, message);
    }
    let _ = transport.flush();
}

pub(crate) fn room_is_reapable(
    cfg: &engine_config::GameConfig,
    name: &str,
    udp_peers: usize,
) -> bool {
    !cfg.lifecycle.is_persistent(name) && (name != cfg.name.as_str() || udp_peers == 0)
}
