use std::time::{Duration, Instant};

use engine_bus::Bus;
use engine_net::transport::{GameTransport, TransportEvent};

use crate::rooms::{DRAIN_REASON, GameServer};

use crate::host::*;
pub(crate) fn drain_rooms(
    server: &mut GameServer,
    transport: &mut LoopTransport,
    bus: &Bus,
    game_cfg: &engine_config::GameConfig,
    game_id: &str,
) {
    let names: Vec<std::rc::Rc<str>> = server.rooms.keys().cloned().collect();
    for name in &names {
        if let Some(room) = server.rooms.get_mut(name) {
            // Give every room one flush to say goodbye (its broadcast goes
            // out with the sessions-closing frames right after).
            room.guest_on_before_shutdown();
            let history = EventHistory::new(bus, &game_cfg.gameplay);
            flush_room_outbox(room, transport, name, &history);
            close_room_sessions(
                transport,
                room,
                engine_net::close_code::SERVER_SHUTDOWN,
                "server shutting down",
            );
        }
    }

    let mut passes = 0;
    while passes < 10 {
        passes += 1;
        let mut events = Vec::new();
        if transport.poll_events(Duration::ZERO, &mut events).is_err() {
            break;
        }
        let mut any_left = false;
        for event in events {
            let TransportEvent::Disconnected { client_id } = event else {
                continue;
            };
            let _ = transport.take_consented(&client_id);
            let _ = transport.take_room_of(&client_id);
            for room in server.rooms.values_mut() {
                if room.sessions.contains_key(&*client_id) {
                    room.handle_disconnect(&client_id);
                } else if room.spectators.contains(&*client_id) {
                    room.handle_disconnect(&client_id);
                }
            }
        }
        any_left = server
            .rooms
            .values()
            .any(|r| !r.sessions.is_empty() || !r.spectators.is_empty());
        if !any_left {
            break;
        }
    }

    for name in names {
        let Some(mut room) = server.rooms.remove(&name) else {
            continue;
        };
        if let Some(snapshot) = room.export_snapshot() {
            report_snapshot(bus, &room, &name, &snapshot);
            server.save_snapshot(&name, snapshot);
        }
        report_match(
            bus,
            game_id,
            &room,
            &name,
            DRAIN_REASON,
            room.sessions.len() as u32,
        );
        report_ownership(
            bus,
            &engine_config::node_id(),
            game_id,
            &name,
            &room,
            engine_bus::RoomPhase::Closed,
        );
        room.dispose();
        tracing::info!("[server] room '{name}' drained and state saved (shutdown)");
    }
}

#[derive(Debug)]
pub(crate) struct Drain {
    pub(crate) grace: Duration,
    pub(crate) deadline: Option<Instant>,
}

impl Drain {
    pub(crate) fn new(grace: Duration) -> Self {
        Self {
            grace,
            deadline: None,
        }
    }

    pub(crate) fn begin(&mut self, now: Instant) -> bool {
        if self.deadline.is_some() {
            return false;
        }
        self.deadline = Some(now + self.grace);
        true
    }

    pub(crate) fn expired(&self, now: Instant) -> bool {
        self.deadline.is_some_and(|at| now >= at)
    }
}
