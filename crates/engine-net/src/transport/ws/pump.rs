use std::io;
use std::time::{Duration, Instant};

use super::accept::on_listener_event;
use super::client_io::on_client_event;
use super::hooks::Pass;
use super::rfc6455::finish_close;
use super::transport::WsTransport;
use crate::transport::TransportEvent;

impl WsTransport {
    pub(super) fn pump(&mut self, timeout: Duration, ev: &mut Vec<TransportEvent>) -> io::Result<()> {
        self.poll.poll(&mut self.events, Some(timeout))?;

        let now = Instant::now();

        let mut buf = std::mem::take(&mut self.ev);
        std::mem::swap(&mut buf, &mut self.pending_events);
        self.pending_events.clear();

        self.run_heartbeat(now);
        let mut hb_kills = std::mem::take(&mut self.hb_kills);
        for token in hb_kills.iter() {
            if let Some(mut client) = self.clients.remove(token) {
                let cid = client.session_id.clone();
                let _ = self.poll.registry().deregister(&mut client.stream);
                self.client_tokens.remove(&*cid);
                self.client_rooms.remove(&*cid);
                self.client_options.remove(&*cid);
                self.seat_tokens.remove(&*cid);
                buf.push(TransportEvent::Disconnected { client_id: cid });
            }
        }
        hb_kills.clear();
        self.hb_kills = hb_kills;

        let mut clients = std::mem::take(&mut self.clients);
        let mut disconnected = std::mem::take(&mut self.disconnected);
        {
            let events = &self.events;
            let mut pass = Pass {
                poll: &mut self.poll,
                listeners: &mut self.listeners,
                clients: &mut clients,
                client_tokens: &mut self.client_tokens,
                client_rooms: &mut self.client_rooms,
                client_options: &mut self.client_options,
                retained: &self.retained,
                seat_tokens: &mut self.seat_tokens,
                matchmake_hook: &mut self.matchmake_hook,
                room_variants: &self.room_variants,
                http_hook: &mut self.http_hook,
                next_token: &mut self.next_token,
                disconnected: &mut disconnected,
                buf: &mut buf,
                game_port: self.bound_addr.port(),
                udp_port: self.advertised_udp_port,
                schema_descriptor: &self.schema_descriptor,
                allowed_origins: &self.allowed_origins,
                simulated_latency: self.simulated_latency,
            };
            for event in events.iter() {
                let tok = event.token();
                if tok.0 < pass.listeners.len() {
                    on_listener_event(&mut pass, tok.0, now)?;
                } else {
                    on_client_event(&mut pass, event, now);
                }
            }
        }

        for token in disconnected.drain(..) {
            if let Some(mut client) = clients.remove(&token) {
                let cid = client.session_id.clone();

                if client.consented {
                    self.consented.insert(cid.to_string());
                }
                let _ = self.poll.registry().deregister(&mut client.stream);
                finish_close(client);
                self.client_tokens.remove(&*cid);
                self.client_rooms.remove(&*cid);
                self.client_options.remove(&*cid);
                self.seat_tokens.remove(&*cid);
                buf.push(TransportEvent::Disconnected { client_id: cid });
            }
        }
        self.clients = clients;
        self.disconnected = disconnected;

        self.flush_delayed_pings(now);
        if self.flush_dirty {
            let _ = self.pump_flush();
        }

        ev.append(&mut buf);
        buf.clear();
        self.ev = buf;
        Ok(())
    }
}
