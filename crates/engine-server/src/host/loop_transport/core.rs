use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use engine_net::transport::webrtc::contract::Inbound;
use engine_net::transport::{GameTransport, WebRtcServer, WebRtcTransport};

use super::state::SeatEntry;
use crate::host::*;
impl LoopTransport {
    pub(crate) fn new(
        rtc: WebRtcTransport,
        quic: Option<engine_net::QuicTransport>,
        ws: Option<engine_net::transport::WsTransport>,
    ) -> Self {
        let local_addr = rtc.local_addr().ok();
        Self {
            rtc,
            quic,
            ws,
            seats: HashMap::new(),
            consented: std::collections::HashSet::new(),
            retained: HashMap::new(),
            inbound: Vec::new(),
            latency: Duration::ZERO,
            packet_loss_rate: 0.0,
            loss_rng: 0x9e3779b9,
            dropped_packets: 0,
            delayed_out: VecDeque::new(),
            reliable_seqs: HashMap::new(),
            delayed_in: VecDeque::new(),
            local_addr,
        }
    }

    pub fn set_latency(&mut self, one_way: Duration) {
        self.latency = one_way;
        if let Some(ws) = self.ws.as_mut() {
            ws.set_simulated_latency(one_way * 2);
        }
    }

    pub fn set_packet_loss_rate(&mut self, rate: f64) {
        self.packet_loss_rate = rate.clamp(0.0, 1.0);
    }

    pub fn packet_loss_rate(&self) -> f64 {
        self.packet_loss_rate
    }

    pub fn should_drop_packet(&mut self) -> bool {
        if self.packet_loss_rate <= 0.0 {
            return false;
        }
        self.loss_rng ^= self.loss_rng << 13;
        self.loss_rng ^= self.loss_rng >> 17;
        self.loss_rng ^= self.loss_rng << 5;
        let roll = (self.loss_rng as f64) / (u32::MAX as f64);
        let drop = roll < self.packet_loss_rate;
        if drop {
            self.dropped_packets += 1;
        }
        drop
    }

    pub fn register_seat(&mut self, session_id: &str, room: &str, options: &str) {
        self.seats.insert(
            session_id.to_string(),
            SeatEntry {
                room: Arc::from(room),
                options: Arc::from(options),
            },
        );
    }

    pub fn room_of(&self, client_id: &str) -> Option<Arc<str>> {
        if let Some(room) = self.ws.as_ref().and_then(|ws| ws.room_of_shared(client_id)) {
            return Some(room);
        }
        self.seats.get(client_id).map(|s| s.room.clone())
    }

    pub fn options_of(&self, client_id: &str) -> Option<Arc<str>> {
        if let Some(opts) = self
            .ws
            .as_ref()
            .and_then(|ws| ws.options_of_shared(client_id))
        {
            return Some(opts);
        }
        self.seats.get(client_id).map(|s| s.options.clone())
    }

    pub fn take_consented(&mut self, client_id: &str) -> bool {
        let ws_consented = self
            .ws
            .as_mut()
            .is_some_and(|ws| ws.take_consented(client_id));
        self.consented.remove(client_id) || ws_consented
    }

    pub fn retain_session(&mut self, room_id: &str, session_id: &str) {
        if let Some(ws) = self.ws.as_mut() {
            ws.retain_session(room_id, session_id);
        }
        self.retained.insert(
            format!("{}:{}", room_id, session_id),
            session_id.to_string(),
        );
    }

    pub fn take_room_of(&mut self, client_id: &str) -> Option<Arc<str>> {
        self.reliable_seqs.remove(client_id);
        self.seats.remove(client_id).map(|s| s.room)
    }

    pub fn dropped_frames(&self) -> u64 {
        self.rtc.dropped_frames()
    }

    pub fn rtc_server(&mut self) -> &mut WebRtcServer {
        self.rtc.server_mut()
    }

    pub fn is_rtc_connected(&self, session: &str) -> bool {
        self.rtc.server().is_connected(session)
    }

    pub fn take_inbound(&mut self) -> Vec<(Arc<str>, Inbound)> {
        std::mem::take(&mut self.inbound)
    }
}
