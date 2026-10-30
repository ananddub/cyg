//! Colyseus frame processing: decode the WS frames in a client's receive
//! buffer and dispatch each complete wire message.

use std::time::Instant;

use bytes::Bytes;

use super::client::WsClient;
use super::protocol::{
    PROTO_JOIN_ROOM, PROTO_LEAVE_ROOM, PROTO_PING, PROTO_ROOM_DATA, PROTO_ROOM_DATA_BYTES,
    PROTO_ROOM_REQUEST, PROTOCOL_CODE_MASK, close_code,
};
use super::rfc6455::{
    MAX_INBOUND_FRAME_SIZE, next_ws_frame, queue_close_frame, queue_message, queue_ws_control,
};
use crate::framing::OpCode;
use crate::transport::TransportEvent;

/// Decode and dispatch all complete WebSocket frames in the client's receive
/// buffer. Returns `true` when the client must be disconnected (LEAVE_ROOM or
/// a client-initiated WS close). Frames are taken one at a time straight off
/// the buffer — no intermediate frame list.
pub(super) fn process_ws_frames(
    client: &mut WsClient,
    events_out: &mut Vec<TransportEvent>,
    now: Instant,
) -> bool {
    while let Some(frame) = next_ws_frame(&mut client.recv_buf) {
        let (ws_opcode, fin, payload) = match frame {
            Ok(frame) => frame,
            Err(()) => {
                // Peer declared a frame over MAX_INBOUND_FRAME_SIZE: policy
                // violation — close with 1009 (Message Too Big) and drop.
                client.consented = true;
                queue_close_frame(client, 1009, "frame too large");
                return true;
            }
        };
        // Any inbound frame proves the client is alive: refresh the heartbeat deadline (some WebSocket stacks answer app traffic but delay pongs).
        client.last_activity = now;
        client.ping_sent_at = None;
        client.missed_pings = 0;

        match ws_opcode {
            0x8 => {
                // Client-initiated close: echo a close frame, then drop.
                client.consented = true;
                queue_close_frame(client, close_code::CONSENTED, "leave");
                return true;
            }
            0x9 => {
                // RFC 6455 ping -> pong with the same payload.
                queue_ws_control(client, 0xA, &payload);
                continue;
            }
            0xA => {
                // Pong: heartbeat already cleared above.
                continue;
            }
            0x0 => {
                // Continuation fragment. Same 1 MiB cap as single frames — an
                // endless fin=0 stream must not accumulate memory forever.
                if client.frag_opcode.is_some() {
                    if client.frag_buf.len() + payload.len()
                        > super::rfc6455::MAX_INBOUND_FRAME_SIZE
                    {
                        client.consented = true;
                        queue_close_frame(client, 1009, "message too large");
                        return true;
                    }
                    client.frag_buf.extend_from_slice(&payload);
                    if fin {
                        let msg = client.frag_buf.split().freeze();
                        client.frag_opcode = None;
                        if dispatch_message(client, msg, events_out) {
                            return true;
                        }
                    }
                }
                continue;
            }
            0x1 | 0x2 => {
                if fin {
                    if dispatch_message(client, payload, events_out) {
                        return true;
                    }
                } else {
                    client.frag_opcode = Some(ws_opcode);
                    client.frag_buf.clear();
                    client.frag_buf.extend_from_slice(&payload);
                }
                continue;
            }
            _ => continue, // unknown WS opcode — ignore
        }
    }
    false
}

/// Dispatch one complete wire protocol message. `msg[0]` is the protocol
/// byte (modifier bits stripped via `PROTOCOL_CODE_MASK` before dispatch).
/// `msg` is a zero-copy view of the socket receive buffer.
/// Returns `true` when the client must be disconnected (LEAVE_ROOM).
fn dispatch_message(
    client: &mut WsClient,
    msg: Bytes,
    events_out: &mut Vec<TransportEvent>,
) -> bool {
    if msg.is_empty() {
        return false;
    }
    let code = msg[0] & PROTOCOL_CODE_MASK;

    match code {
        PROTO_JOIN_ROOM => {
            // Client ack of the server-initiated join — nothing to do; the handshake was already sent proactively on upgrade.
        }
        PROTO_LEAVE_ROOM => {
            // Client leave: answer close 4000 CONSENTED, then drop.
            client.consented = true;
            queue_close_frame(client, close_code::CONSENTED, "leave");
            return true;
        }
        PROTO_ROOM_DATA => {
            events_out.push(TransportEvent::Packet {
                client_id: client.session_id.clone(),
                opcode: OpCode::PlayerInput,
                payload: msg.slice(1..),
            });
        }
        PROTO_ROOM_DATA_BYTES => {
            events_out.push(TransportEvent::Packet {
                client_id: client.session_id.clone(),
                opcode: OpCode::RoomDataBytes,
                payload: msg.slice(1..),
            });
        }
        PROTO_PING => {
            // App-level ping: echo the payload back so the client can measure RTT (payload carries the client's sequence number).
            queue_message(client, PROTO_PING, &msg[1..]);
        }
        PROTO_ROOM_REQUEST => {
            events_out.push(TransportEvent::Packet {
                client_id: client.session_id.clone(),
                opcode: OpCode::RoomRequest,
                payload: msg.slice(1..),
            });
        }
        _ => {
            // Unknown/defensive: surface the full frame to the game layer.
            events_out.push(TransportEvent::Packet {
                client_id: client.session_id.clone(),
                opcode: OpCode::Custom,
                payload: msg,
            });
        }
    }
    false
}
