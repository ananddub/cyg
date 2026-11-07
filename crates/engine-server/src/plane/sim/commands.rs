use engine_bus::{FrameKind, OutFrame, SimCommand};

use crate::plane::sim::helpers::{bind_peer, drop_peer};
use crate::plane::sim::state::Plane;

impl Plane {
    pub(super) fn drain_commands(&mut self) {
        while let Ok(cmd) = self.bus.commands_rx.try_recv() {
            match cmd {
                SimCommand::BindPeer {
                    peer_id,
                    room: wanted,
                    reply,
                } => {
                    let id = bind_peer(
                        &mut self.room,
                        peer_id.clone(),
                        &wanted,
                        &self.cfg.room_name,
                        self.cfg.max_players,
                        &mut self.session_of,
                    );
                    let _ = reply.send(id);
                    if id >= 0 {
                        tracing::info!(
                            "[sim] {peer_id} bound to room '{}' as player {id}",
                            self.cfg.room_name
                        );
                    } else {
                        tracing::info!("[sim] bind rejected for {peer_id}: room mismatch or full");
                    }
                }
                SimCommand::DropPeer { peer_id } => {
                    drop_peer(
                        &mut self.room,
                        &peer_id,
                        &mut self.browser_players,
                        &mut self.session_of,
                    );
                }
                SimCommand::Event {
                    room: wanted,
                    target,
                    bytes,
                } => {
                    if wanted != self.cfg.room_name || self.room.sessions.is_empty() {
                        continue;
                    }
                    let targets: Vec<std::sync::Arc<str>> = match target {
                        Some(t) if self.room.sessions.contains_key(t.as_str()) => vec![std::sync::Arc::from(t)],
                        Some(_) => vec![],
                        None => self.room.sessions.keys().cloned().collect(),
                    };
                    for t in targets {
                        let frame = OutFrame {
                            peer_id: t.to_string(),
                            kind: FrameKind::Reliable,
                            bytes: bytes.clone(),
                        };
                        if self.browser_players.contains(&self.room.sessions[t.as_ref()]) {
                            let _ = self.bus.deltas_tx.try_send(frame);
                        } else if !self.udp.send_frame(&frame) {
                            self.native_send_failed += 1;
                        }
                    }
                }
            }
        }
    }
}
