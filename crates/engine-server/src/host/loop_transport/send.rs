use std::time::Instant;

use engine_net::framing::OpCode;
use engine_net::transport::GameTransport;

use crate::netcode;

use crate::host::*;
impl LoopTransport {
    /// Zero-copy WS delta: the body is the room's single shared allocation.
    /// True when the WS lane exists AND the shared-body gate is on. Hot
    /// callers consult this BEFORE building Arc bodies — building them and
    /// then getting a `false` back allocated ~76k Arcs/s for nothing at 10k.
    pub fn shared_delta_enabled(&self) -> bool {
        static GATE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *GATE.get_or_init(|| std::env::var_os("CYG_SHARED_DELTA").is_some()) && self.ws.is_some()
    }

    /// Returns false when the session is not on the WS lane — callers fall
    /// back to the wrapped/copy path (WebRTC, spectators, culled views).
    pub fn send_delta_shared(
        &mut self,
        session: &str,
        your_id: i64,
        body: std::sync::Arc<[u8]>,
        acked_seq: u16,
        server_tick: u32,
        client_stamp: u16,
    ) -> bool {
        // EXPERIMENTAL: the shared-body queue leaks under sustained load
        // (RSS regression caught at ~3000 CCU) — keep it off by default and
        // opt in with CYG_SHARED_DELTA=1 until the leak is fixed.
        if !self.shared_delta_enabled() {
            return false;
        }
        // Wire frame (RoomDataBytes lane, RFC6455): [0x82][len][code] +
        // payload = [0xA5 "delta"][your_id][schema body][acked u16][tick
        // u32][client_stamp u16] — the Phase D RTT echo tail.
        let payload_len = 1 + 14 + body.len() + 8;
        let mut head: Vec<u8> = if payload_len <= 125 {
            let mut h = Vec::with_capacity(17);
            h.push(0x82);
            h.push(payload_len as u8);
            h.push(0x11); // ws room-data lane wire opcode (17)
            h
        } else if payload_len <= 65535 {
            let mut h = Vec::with_capacity(19);
            h.push(0x82);
            h.push(126);
            h.extend_from_slice(&(payload_len as u16).to_be_bytes());
            h.push(0x11); // ws room-data lane wire opcode (17)
            h
        } else {
            return false; // oversized for this lane: caller uses the wrapped path
        };
        head.extend_from_slice(&[0xA5, b'd', b'e', b'l', b't', b'a']);
        head.extend_from_slice(&your_id.to_le_bytes());
        let mut tail = [0u8; 8];
        tail[..2].copy_from_slice(&acked_seq.to_le_bytes());
        tail[2..6].copy_from_slice(&server_tick.to_le_bytes());
        tail[6..].copy_from_slice(&client_stamp.to_le_bytes());
        match self.ws.as_mut() {
            Some(ws) => ws.queue_shared_delta(session, &head, body, &tail),
            None => false,
        }
    }

    pub fn send_delta_framed(&mut self, session: &str, seq: u16, tick: u32, body: &[u8]) -> usize {
        let count = engine_webrtc_protocol::delta_fragment_count(body.len()).unwrap_or(0) as usize;
        if self.should_drop_packet() {
            return count;
        }
        if !self.latency.is_zero() {
            if !self.rtc.server().is_connected(session) {
                return 0;
            }
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

    /// Sends one already-wrapped state frame on the reliable lane. Volatile
    /// fields opt a room into this path so inventory/score patches are never
    /// lost together with a dropped position frame.
    pub fn send_state_reliable(
        &mut self,
        session: &str,
        frame: &[u8],
        tick: u32,
    ) -> std::io::Result<()> {
        if self.ws_knows(session) {
            return self
                .ws
                .as_mut()
                .unwrap()
                .send_to(session, OpCode::RoomDataBytes, frame);
        }
        if self.rtc.server().is_connected(session) {
            let seq = self.next_reliable_seq(session);
            let sent = crate::netcode::send_delta_framed_reliable(
                self.rtc.server_mut(),
                session,
                seq,
                tick,
                frame,
            );
            self.rtc.flush()?;
            if sent == 0 {
                return Err(std::io::Error::other(
                    "reliable state frame was refused by the transport",
                ));
            }
            return Ok(());
        }
        self.rtc.send_to(session, OpCode::RoomDataBytes, frame)
    }

    /// Sends a volatile-only patch on the lossy lane. On the WebSocket lane
    /// there is no lossy sub-channel, so it rides the ordered socket; the
    /// client still applies it as a separate, tick-gated stream.
    pub fn send_volatile(&mut self, session: &str, frame: &[u8]) -> std::io::Result<()> {
        if self.ws_knows(session) {
            return self
                .ws
                .as_mut()
                .unwrap()
                .send_to(session, OpCode::RoomDataBytes, frame);
        }
        if self.rtc.server().is_connected(session) {
            const HEADER: usize = 7; // 0xA7 + tick u32 + idx u8 + total u8
            let chunk_len = engine_webrtc_protocol::MAX_FRAME - HEADER;
            if frame.len() <= engine_webrtc_protocol::MAX_FRAME {
                let sent = self.rtc.server_mut().send_unreliable(session, frame);
                if !sent {
                    tracing::error!(
                        "[server] ⚠️ volatile frame of {} B for {session} refused by the lossy lane",
                        frame.len()
                    );
                }
            } else if frame.len() <= engine_webrtc_protocol::MAX_DELTA_BODY {
                let total = frame.len().div_ceil(chunk_len);
                let tick = frame
                    .get(1..5)
                    .and_then(|bytes| bytes.try_into().ok())
                    .map(u32::from_le_bytes)
                    .unwrap_or(0);
                let mut fragment = Vec::with_capacity(engine_webrtc_protocol::MAX_FRAME);
                for (index, chunk) in frame.chunks(chunk_len).enumerate() {
                    fragment.clear();
                    fragment.push(0xA7);
                    fragment.extend_from_slice(&tick.to_le_bytes());
                    fragment.push(index as u8);
                    fragment.push(total as u8);
                    fragment.extend_from_slice(chunk);
                    if !self
                        .rtc
                        .server_mut()
                        .send_unreliable(session, &fragment)
                    {
                        tracing::error!(
                            "[server] ⚠️ volatile fragment {index}/{total} for {session} refused by the lossy lane"
                        );
                        break;
                    }
                }
            } else {
                tracing::error!(
                    "[server] ⚠️ volatile frame of {} B for {session} exceeds the fragmentation budget",
                    frame.len()
                );
            }
            return self.rtc.flush();
        }
        self.rtc.send_to(session, OpCode::RoomDataBytes, frame)
    }

    fn next_reliable_seq(&mut self, session: &str) -> u16 {
        let seq = self.reliable_seqs.entry(session.to_string()).or_insert(0);
        let value = *seq;
        *seq = seq.wrapping_add(1);
        value
    }

    pub fn send_state(&mut self, client_id: &str, payload: &[u8]) -> std::io::Result<()> {
        if self.should_drop_packet() {
            return Ok(());
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
        self.send_state_now(client_id, payload)
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
