use std::time::{Duration, Instant};

use engine_net::framing::OpCode;
use engine_net::transport::webrtc::contract::Inbound;
use engine_net::transport::{GameTransport, TransportEvent};
use engine_webrtc_protocol::{HEADER_LEN as WS_HEADER_LEN, INPUT_WIRE_LEN as WS_INPUT_WIRE_LEN};

use crate::host::*;
impl LoopTransport {
    pub(crate) fn poll_events_inner(
        &mut self,
        timeout: Duration,
        events: &mut Vec<TransportEvent>,
    ) -> std::io::Result<()> {
        self.rtc.poll_events(timeout, events)?;
        events.retain(|ev| {
            !matches!(
                ev,
                TransportEvent::Packet {
                    opcode: OpCode::PlayerInput,
                    ..
                }
            ) && !matches!(
                ev,
                TransportEvent::Packet {
                    opcode: OpCode::RoomDataBytes,
                    ..
                }
            )
        });
        self.inbound.extend(self.rtc.poll_inbound());
        if let Some(quic) = self.quic.as_mut() {
            quic.poll_events(timeout, events)?;
            self.inbound.extend(quic.poll_inbound());
        }
        if let Some(ws) = self.ws.as_mut() {
            ws.poll_events(timeout, events)?;
            events.retain(|ev| match ev {
                TransportEvent::Packet {
                    client_id,
                    opcode,
                    payload,
                } => match opcode {
                    OpCode::PlayerInput => {
                        const SAMPLE: usize = WS_INPUT_WIRE_LEN - WS_HEADER_LEN;
                        if payload.len() >= 2 + SAMPLE {
                            let seq = u16::from_le_bytes([payload[0], payload[1]]);
                            let mut sample = [0u8; SAMPLE];
                            sample.copy_from_slice(&payload[2..2 + SAMPLE]);
                            self.inbound.push((
                                client_id.clone(),
                                Inbound::Input {
                                    seq,
                                    payload: sample,
                                    resupplied: false,
                                },
                            ));
                        }
                        false
                    }
                    OpCode::RoomDataBytes => {
                        self.inbound.push((
                            client_id.clone(),
                            Inbound::App {
                                data: payload.clone(),
                            },
                        ));
                        false
                    }
                    _ => true,
                },
                _ => true,
            });
        }
        let now = Instant::now();
        self.flush_delayed_out(now);
        if self.packet_loss_rate > 0.0 {
            let rate = self.packet_loss_rate;
            let rng = &mut self.loss_rng;
            let dropped = &mut self.dropped_packets;
            self.inbound.retain(|(_, msg)| match msg {
                Inbound::Input { .. } => {
                    *rng ^= *rng << 13;
                    *rng ^= *rng >> 17;
                    *rng ^= *rng << 5;
                    let roll = (*rng as f64) / (u32::MAX as f64);
                    let drop = roll < rate;
                    if drop {
                        *dropped += 1;
                    }
                    !drop
                }
                _ => true,
            });
        }
        if !self.latency.is_zero() {
            for (id, msg) in self.inbound.drain(..) {
                self.delayed_in.push_back((now + self.latency, id, msg));
            }
        }
        while let Some((due, ..)) = self.delayed_in.front() {
            if *due > now {
                break;
            }
            let (_, id, msg) = self.delayed_in.pop_front().unwrap();
            self.inbound.push((id, msg));
        }
        Ok(())
    }
}
