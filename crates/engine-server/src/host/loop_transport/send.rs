use std::time::Instant;

use engine_net::framing::OpCode;
use engine_net::transport::GameTransport;

use crate::netcode;

use crate::host::*;
impl LoopTransport {

    pub fn send_delta_framed(&mut self, session: &str, seq: u16, tick: u32, body: &[u8]) -> usize {
        if !self.latency.is_zero() {
            if !self.rtc.server().is_connected(session) {
                return 0;
            }
            let count =
                engine_webrtc_protocol::delta_fragment_count(body.len()).unwrap_or(0) as usize;
            self.delayed_out.push_back((
                Instant::now() + self.latency,
                DelayedOut::Delta {
                    session: session.to_string(),
                    seq,
                    tick,
                    body: body.to_vec(),
                },
            ));
            return count;
        }
        netcode::send_delta_framed(self.rtc.server_mut(), session, seq, tick, body)
    }

    pub fn send_state(&mut self, client_id: &str, payload: &[u8]) -> std::io::Result<()> {
        if self.ws_knows(client_id) {
            return self.ws.as_mut().unwrap().send_state(client_id, payload);
        }
        if !self.latency.is_zero() {
            self.delayed_out.push_back((
                Instant::now() + self.latency,
                DelayedOut::State {
                    client_id: client_id.to_string(),
                    payload: payload.to_vec(),
                },
            ));
            return Ok(());
        }
        self.rtc.send_state(client_id, payload)
    }

    pub fn send_to_many(
        &mut self,
        client_ids: &[&str],
        opcode: OpCode,
        payload: &[u8],
    ) -> std::io::Result<()> {
        for &cid in client_ids {
            let _ = self.rtc.send_to(cid, opcode, payload);
        }
        Ok(())
    }

}
