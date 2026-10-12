//! Minimal Colyseus SDK end-to-end server with live schema state.
//!
//! Run with: cargo run --release -p engine-net --example colyseus_e2e
//!
//! - Accepts @colyseus/sdk clients (matchmake + join handshake automatic).
//! - Sends a ROOM_STATE full snapshot to each newly joined client.
//! - Broadcasts ROOM_STATE_PATCH deltas ~10x/sec: `tick` advances and
//!   player `p1` orbits the origin.
//!
//! Client schema must match this layout (field index order matters!):
//! ```js
//! class Player extends Schema {}
//! defineTypes(Player, { x: "number", y: "number" });   // indices 0, 1
//! class GameState extends Schema {}
//! defineTypes(GameState, { players: { map: Player }, tick: "number" }); // 0, 1
//! ```

use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use engine_net::colyseus_state::ColyseusRoomState;
use engine_net::framing::OpCode;
use engine_net::transport::{ColyseusWsTransport, GameTransport, TransportEvent};

fn main() {
    let mut transport =
        ColyseusWsTransport::new("127.0.0.1:7777".parse::<SocketAddr>().unwrap()).expect("bind");
    eprintln!(
        "[server] Colyseus WS transport listening on {}",
        transport.local_addr().unwrap()
    );

    let mut state = ColyseusRoomState::new();
    state.add_player("p1", 10.5, -3.25, 0.0);
    let mut known_clients: HashMap<String, ()> = HashMap::new();

    let mut last_broadcast = Instant::now();
    let broadcast_interval = Duration::from_millis(100);
    let deadline = Instant::now() + Duration::from_secs(60);

    while Instant::now() < deadline {
        let events = transport
            .poll_events(Duration::from_millis(5))
            .expect("poll");

        for event in events {
            match event {
                TransportEvent::Connected { client_id, .. } => {
                    eprintln!("[server] connected: {client_id}");
                    // Full snapshot for the fresh client (skip matchmake HTTP
                    // connections — they never become transport clients).
                    let snapshot = state.snapshot();
                    eprintln!("[server] snapshot ({}B): {:02X?}", snapshot.len(), snapshot);
                    transport
                        .send_to(&client_id, OpCode::StateSnapshot, &snapshot)
                        .expect("snapshot");
                    known_clients.insert(client_id, ());
                    let _ = transport.flush();
                }
                TransportEvent::Disconnected { client_id } => {
                    eprintln!("[server] disconnected: {client_id}");
                }
                TransportEvent::Packet {
                    client_id,
                    opcode,
                    payload,
                } => {
                    eprintln!(
                        "[server] packet from {client_id} (op {opcode:?}): {:?}",
                        payload
                    );
                }
            }
        }

        if last_broadcast.elapsed() >= broadcast_interval {
            last_broadcast = Instant::now();
            state.advance_tick();
            let t = state.tick() as f64;
            state.update_player("p1", 10.5 + t * 0.5, -3.25 + (t * 0.1).sin() * 5.0, t * 10.0);

            let patch = state.patch();
            if !patch.is_empty() {
                transport
                    .broadcast(OpCode::StateDelta, &patch)
                    .expect("broadcast patch");
            }
            transport.flush().expect("flush");
        }

        transport.flush().expect("flush");
    }
}
