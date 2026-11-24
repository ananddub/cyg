use std::time::Instant;

use engine_net::framing::OpCode;
use engine_net::transport::GameTransport;

use crate::netcode;

use crate::host::*;
impl LoopTransport {
    pub(crate) fn ws_knows(&self, client_id: &str) -> bool {
        self.ws
            .as_ref()
            .is_some_and(|ws| ws.room_of(client_id).is_some())
    }

    pub(crate) fn flush_delayed_out(&mut self, now: Instant) {
        while let Some((due, _)) = self.delayed_out.front() {
            if *due > now {
                break;
            }
            let (_, msg) = self.delayed_out.pop_front().unwrap();
            match msg {
                DelayedOut::Op {
                    client_id,
                    opcode,
                    payload,
                } => {
                    let _ = self.send_to_now(&client_id, opcode, &payload);
                }
                DelayedOut::State { client_id, payload } => {
                    let _ = self.send_state_now(&client_id, &payload);
                }
                DelayedOut::Delta {
                    session,
                    seq,
                    tick,
                    body,
                } => {
                    let _ = netcode::send_delta_framed(
                        self.rtc.server_mut(),
                        &session,
                        seq,
                        tick,
                        &body,
                    );
                }
            }
        }
    }

    pub(crate) fn send_to_now(
        &mut self,
        client_id: &str,
        opcode: OpCode,
        payload: &[u8],
    ) -> std::io::Result<()> {
        if self.quic.as_ref().is_some_and(|q| q.knows(client_id)) {
            return self
                .quic
                .as_mut()
                .unwrap()
                .send_to(client_id, opcode, payload);
        }
        if self.ws_knows(client_id) {
            return self
                .ws
                .as_mut()
                .unwrap()
                .send_to(client_id, opcode, payload);
        }
        self.rtc.send_to(client_id, opcode, payload)
    }

    pub(crate) fn send_state_now(
        &mut self,
        client_id: &str,
        payload: &[u8],
    ) -> std::io::Result<()> {
        if self.ws_knows(client_id) {
            return self.ws.as_mut().unwrap().send_state(client_id, payload);
        }
        self.rtc.send_state(client_id, payload)
    }
}
