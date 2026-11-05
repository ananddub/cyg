
use std::time::{Duration, Instant};

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

pub(super) fn process_ws_frames(
    client: &mut WsClient,
    events_out: &mut Vec<TransportEvent>,
    now: Instant,
    simulated_latency: Duration,
) -> bool {
    while let Some(frame) = next_ws_frame(&mut client.recv_buf) {
        let (ws_opcode, fin, payload) = match frame {
            Ok(frame) => frame,
            Err(()) => {

                client.consented = true;
                queue_close_frame(client, 1009, "frame too large");
                return true;
            }
        };

        client.last_activity = now;
        client.ping_sent_at = None;
        client.missed_pings = 0;

        match ws_opcode {
            0x8 => {

                client.consented = true;
                queue_close_frame(client, close_code::CONSENTED, "leave");
                return true;
            }
            0x9 => {

                queue_ws_control(client, 0xA, &payload);
                continue;
            }
            0xA => {

                continue;
            }
            0x0 => {

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
                        if dispatch_message(client, msg, events_out, now, simulated_latency) {
                            return true;
                        }
                    }
                }
                continue;
            }
            0x1 | 0x2 => {
                if fin {
                    if dispatch_message(client, payload, events_out, now, simulated_latency) {
                        return true;
                    }
                } else {
                    client.frag_opcode = Some(ws_opcode);
                    client.frag_buf.clear();
                    client.frag_buf.extend_from_slice(&payload);
                }
                continue;
            }
            _ => continue,
        }
    }
    false
}

fn dispatch_message(
    client: &mut WsClient,
    msg: Bytes,
    events_out: &mut Vec<TransportEvent>,
    now: Instant,
    simulated_latency: Duration,
) -> bool {
    if msg.is_empty() {
        return false;
    }
    let code = msg[0] & PROTOCOL_CODE_MASK;

    match code {
        PROTO_JOIN_ROOM => {

        }
        PROTO_LEAVE_ROOM => {

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
            if simulated_latency.is_zero() {
                queue_message(client, PROTO_PING, &msg[1..]);
            } else {
                client.delayed_pings.push((now + simulated_latency, msg.slice(1..)));
            }
        }
        PROTO_ROOM_REQUEST => {
            events_out.push(TransportEvent::Packet {
                client_id: client.session_id.clone(),
                opcode: OpCode::RoomRequest,
                payload: msg.slice(1..),
            });
        }
        _ => {

            events_out.push(TransportEvent::Packet {
                client_id: client.session_id.clone(),
                opcode: OpCode::Custom,
                payload: msg,
            });
        }
    }
    false
}
