//! # Native ECS Room Backend
//!
//! The 7-pillar simulation stack wired into the production room loop:
//! `bevy_ecs` (TickRunner) + Rapier 3D (ECS-synced systems) + `RoomRng`
//! (deterministic, seeded per room) + OXCL cooked arena colliders.
//!
//! Selected with a room name containing `ecs` (e.g. joining room `ecs-arena`).
//! The host loop ticks it like any other backend: inputs in, one delta buffer
//! out per tick, broadcast through the same outbox the WASM guests use.
//!
//! Delta wire format (game-agnostic TLV, one entry per changed entity):
//! `[tag u8=0x01][len u8=24][id u64 LE][x f32 LE][y f32 LE][z f32 LE][hp f32 LE]`

use std::collections::HashMap;

use bevy_ecs::prelude::*;
use engine_core::{
    EntityId, Health, PendingDespawn, PlayerControlled, Position3D, RoomRng, TickRunner, Velocity,
};
use engine_physics::{
    step_rapier_3d, sync_ecs_to_rapier_3d, sync_rapier_to_ecs_3d, sync_velocity_to_rapier_3d,
    CookedMesh, RapierWorld3D,
};
use rapier3d::prelude as r3d;

/// WASD flag bits shared with the demo guests (W=1 S=2 A=4 D=8).
pub mod input_flags {
    pub const W: u8 = 1;
    pub const S: u8 = 2;
    pub const A: u8 = 4;
    pub const D: u8 = 8;
}

const MOVE_SPEED: f32 = 240.0; // units / second
const SPAWN_HEIGHT: f32 = 1.0;
const PLAYER_RADIUS: f32 = 0.4;
const PLAYER_HALF_HEIGHT: f32 = 0.5;

/// Identifies the room inside the ECS world (for game systems that need it).
#[derive(Resource)]
pub struct RoomSeed {
    pub room_name: String,
}

/// One session's held input flags, applied to its avatar each tick.
#[derive(Resource, Default)]
pub struct SessionInputs {
    /// ECS `EntityId.0` -> WASD flags byte.
    flags: HashMap<u32, u8>,
}

impl SessionInputs {
    fn set(&mut self, entity_id: u32, flags: u8) {
        self.flags.insert(entity_id, flags);
    }
    fn remove(&mut self, entity_id: u32) {
        self.flags.remove(&entity_id);
    }
}

fn apply_inputs(mut query: Query<(&EntityId, &mut Velocity)>, inputs: Res<SessionInputs>) {
    for (eid, mut vel) in &mut query {
        let Some(&flags) = inputs.flags.get(&eid.0) else {
            continue;
        };
        vel.vx = if flags & input_flags::D != 0 {
            MOVE_SPEED
        } else if flags & input_flags::A != 0 {
            -MOVE_SPEED
        } else {
            0.0
        };
        vel.vz = if flags & input_flags::S != 0 {
            MOVE_SPEED
        } else if flags & input_flags::W != 0 {
            -MOVE_SPEED
        } else {
            0.0
        };
        // vy stays physics-owned (gravity / jumps).
    }
}

/// ECS + physics + RNG state of one room. `RapierWorld3D` and `RoomRng` live
/// as ECS resources — the single source of truth the schedule's systems use.
pub struct EcsRoomState {
    runner: TickRunner,
    /// Session -> ECS entity (the player's avatar).
    entities: HashMap<String, Entity>,
    /// Session -> ECS `EntityId.0` (Rapier binding + input bookkeeping key).
    entity_ids: HashMap<String, u32>,
    next_entity_id: u32,
    /// Fixed rapier bindings used to tear bodies down on leave.
    handles: HashMap<u32, r3d::RigidBodyHandle>,
}

impl EcsRoomState {
    /// Build the room world: physics resource, deterministic RNG, and the
    /// fixed sync schedule. `arena` (from `ox-game cook` -> `arena.oxcl`)
    /// is baked in as a static trimesh when present.
    pub fn new(room_name: &str, arena: Option<&CookedMesh>) -> Self {
        let mut runner = TickRunner::new();
        let mut rapier = RapierWorld3D::default();

        // Ground plane so a room without cooked geometry still plays.
        let ground = r3d::ColliderBuilder::cuboid(50.0, 0.5, 50.0)
            .translation(r3d::Vector::new(0.0, -0.5, 0.0))
            .build();
        rapier.collider_set.insert(ground);

        if let Some(mesh) = arena {
            rapier.add_oxcl_mesh(mesh);
        }

        runner.world.insert_resource(rapier);
        runner
            .world
            .insert_resource(RoomRng::from_seed_str(room_name));
        runner.world.insert_resource(RoomSeed {
            room_name: room_name.to_string(),
        });
        runner.world.insert_resource(SessionInputs::default());

        // Input -> ECS, ECS -> Rapier, simulate, Rapier -> ECS: one ordered,
        // single-threaded schedule. Changed<Position3D> tracking then limits
        // the delta to entities that actually moved.
        runner.add_system((
            apply_inputs,
            sync_ecs_to_rapier_3d,
            sync_velocity_to_rapier_3d,
            step_rapier_3d,
            sync_rapier_to_ecs_3d,
        ));

        Self {
            runner,
            entities: HashMap::new(),
            entity_ids: HashMap::new(),
            next_entity_id: 1,
            handles: HashMap::new(),
        }
    }

    #[allow(dead_code)]
    fn rapier(&self) -> &RapierWorld3D {
        self.runner.world.resource::<RapierWorld3D>()
    }

    fn rapier_mut(&mut self) -> Mut<'_, RapierWorld3D> {
        self.runner.world.resource_mut::<RapierWorld3D>()
    }

    /// Spawn the avatar entity for a joining session and bind it to physics.
    pub fn add_player(&mut self, session: &str, id: i64, x: f64, y: f64) {
        let entity_id = self.next_entity_id;
        self.next_entity_id += 1;

        // into_inner: disjoint-field borrows the checker can see.
        let rapier = self.rapier_mut().into_inner();
        let body = r3d::RigidBodyBuilder::dynamic()
            .translation(r3d::Vector::new(x as f32, SPAWN_HEIGHT, y as f32))
            .lock_rotations()
            .build();
        let handle = rapier.rigid_body_set.insert(body);
        let collider = r3d::ColliderBuilder::capsule_y(PLAYER_HALF_HEIGHT, PLAYER_RADIUS).build();
        rapier
            .collider_set
            .insert_with_parent(collider, handle, &mut rapier.rigid_body_set);
        rapier.bind_entity(entity_id, handle);
        let _ = rapier;

        let entity = self
            .runner
            .world
            .spawn((
                EntityId(entity_id),
                Position3D {
                    x: x as f32,
                    y: SPAWN_HEIGHT,
                    z: y as f32,
                },
                Velocity::default(),
                Health::new(100.0),
                PlayerControlled { client_id: id as u64 },
            ))
            .id();

        self.entities.insert(session.to_string(), entity);
        self.entity_ids.insert(session.to_string(), entity_id);
        self.handles.insert(entity_id, handle);
        self.runner.world.resource_mut::<SessionInputs>().set(entity_id, 0);
    }

    /// Queue a leaving session's avatar for despawn at the end of the tick
    /// (`PendingDespawn` is the engine's cleanup contract) and drop the
    /// physics binding immediately so the ghost stops simulating.
    pub fn remove_player(&mut self, session: &str) {
        if let Some(&entity) = self.entities.get(session) {
            self.runner.world.entity_mut(entity).insert(PendingDespawn);
        }
        if let Some(entity_id) = self.entity_ids.remove(session) {
            if let Some(handle) = self.handles.remove(&entity_id) {
                let rapier = self.rapier_mut().into_inner();
                rapier.rigid_body_set.remove(
                    handle,
                    &mut rapier.island_manager,
                    &mut rapier.collider_set,
                    &mut rapier.impulse_joint_set,
                    &mut rapier.multibody_joint_set,
                    &mut rapier.soft_body_set,
                    true,
                );
                rapier.entity_handles.remove(&entity_id);
            }
            self.runner.world.resource_mut::<SessionInputs>().remove(entity_id);
        }
        self.entities.remove(session);
    }

    /// Record a session's held WASD flags; applied on the next tick.
    pub fn apply_input(&mut self, session: &str, data: &[u8]) {
        let Some(&entity_id) = self.entity_ids.get(session) else {
            return;
        };
        let flags = data.first().copied().unwrap_or(0);
        self.runner
            .world
            .resource_mut::<SessionInputs>()
            .set(entity_id, flags);
    }

    /// Run one fixed tick and serialize every entity whose `Position3D` or
    /// `Health` changed into the TLV delta buffer.
    pub fn tick(&mut self, _dt: f64) -> Vec<u8> {
        self.runner.tick();

        let mut query = self.runner.world.query_filtered::<(
            &EntityId,
            &Position3D,
            &Health,
        ), Changed<Position3D>>();
        let mut delta = Vec::with_capacity(64);
        for (eid, pos, hp) in query.iter(&self.runner.world) {
            // [tag][len=24][id u64][x f32][y f32][z f32][hp f32]
            delta.push(0x01);
            delta.push(24);
            delta.extend_from_slice(&(eid.0 as u64).to_le_bytes());
            delta.extend_from_slice(&pos.x.to_le_bytes());
            delta.extend_from_slice(&pos.y.to_le_bytes());
            delta.extend_from_slice(&pos.z.to_le_bytes());
            delta.extend_from_slice(&hp.current.to_le_bytes());
        }
        delta
    }

    /// Full-state TLV snapshot (every live avatar, regardless of change
    /// tracking) — used for persistent room snapshots.
    pub fn export_tlv(&mut self) -> Vec<u8> {
        let mut query = self
            .runner
            .world
            .query::<(&EntityId, &Position3D, &Health)>();
        let mut delta = Vec::with_capacity(64);
        for (eid, pos, hp) in query.iter(&self.runner.world) {
            delta.push(0x01);
            delta.push(24);
            delta.extend_from_slice(&(eid.0 as u64).to_le_bytes());
            delta.extend_from_slice(&pos.x.to_le_bytes());
            delta.extend_from_slice(&pos.y.to_le_bytes());
            delta.extend_from_slice(&pos.z.to_le_bytes());
            delta.extend_from_slice(&hp.current.to_le_bytes());
        }
        delta
    }

    /// Deterministic RNG for game systems that prefer direct access over the
    /// ECS resource (same seed, same sequence as `ResMut<RoomRng>` users).
    pub fn rng(&mut self) -> Mut<'_, RoomRng> {
        self.runner.world.resource_mut::<RoomRng>()
    }

    /// The `EntityId.0` of a session's avatar, if present.
    pub fn entity_id_of(&self, session: &str) -> Option<u32> {
        self.entity_ids.get(session).copied()
    }

    /// Raycast into the room's physics world (bullet hit registration).
    pub fn raycast_hit(&mut self, origin: [f32; 3], dir: [f32; 3], max_dist: f32) -> Option<(u32, f32)> {
        self.rapier_mut().into_inner().raycast_hit(origin, dir, max_dist)
    }

    pub fn player_count(&self) -> usize {
        self.entities.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rooms::GameRoom;

    /// A joined player holding D must move +x under the ECS schedule, and the
    /// delta outbox must carry its TLV row through the GameRoom contract.
    #[test]
    fn ecs_room_movement_and_delta_flow() {
        let mut room = GameRoom::new_ecs("ecs-arena", None);
        assert!(room.is_ecs());
        let id = room.join("sess_1", 0.0, 0.0);
        assert_eq!(id, 1);

        // Hold D (flags = 8) for the session's avatar.
        room.ecs_input("sess_1", &[input_flags::D]);
        let mut moved = 0.0f32;
        let mut saw_delta = false;
        for _ in 0..30 {
            room.tick_ecs(1.0 / 60.0);
            for msg in room.drain_outbox() {
                assert_eq!(msg.event, "delta");
                assert_eq!(msg.target, 0, "delta broadcasts room-wide");
                saw_delta = true;
                // Parse the player's TLV row back out.
                let mut i = 0;
                while i + 26 <= msg.payload.len() {
                    assert_eq!(msg.payload[i], 0x01);
                    assert_eq!(msg.payload[i + 1], 24);
                    let eid = u64::from_le_bytes(msg.payload[i + 2..i + 10].try_into().unwrap());
                    let x = f32::from_le_bytes(msg.payload[i + 10..i + 14].try_into().unwrap());
                    if eid == 1 {
                        moved = x;
                    }
                    i += 26;
                }
            }
        }
        assert!(saw_delta, "delta must flow through the outbox");
        assert!(
            moved > 20.0,
            "holding D for half a second must move the avatar +x, got {moved}"
        );
    }

    /// Leaving queues PendingDespawn: the avatar stops shipping deltas.
    #[test]
    fn ecs_room_leave_stops_delta() {
        let mut room = GameRoom::new_ecs("ecs-leave", None);
        room.join("sess_1", 0.0, 0.0);
        room.release_player("sess_1", 1);
        room.tick_ecs(1.0 / 60.0);
        room.drain_outbox();
        // After the despawn tick settles, an idle room ships nothing.
        room.tick_ecs(1.0 / 60.0);
        let drained = room.drain_outbox();
        for msg in &drained {
            let payload = &msg.payload;
            assert!(
                payload.len() < 26,
                "despawned avatar must not appear in deltas"
            );
        }
        assert!(room.ecs.as_ref().unwrap().player_count() == 0);
    }

    /// Same room seed -> bit-for-bit identical RNG sequences (deterministic
    /// procedural generation without network traffic).
    #[test]
    fn room_rng_seed_replay_is_exact() {
        let mut a = EcsRoomState::new("match-ranked-asia-49210", None);
        let mut b = EcsRoomState::new("match-ranked-asia-49210", None);
        let seq_a: Vec<f32> = (0..16).map(|_| a.rng().next_f32()).collect();
        let seq_b: Vec<f32> = (0..16).map(|_| b.rng().next_f32()).collect();
        assert_eq!(seq_a, seq_b);
        let mut c = EcsRoomState::new("match-ranked-asia-49211", None);
        let seq_c: Vec<f32> = (0..16).map(|_| c.rng().next_f32()).collect();
        assert_ne!(seq_a, seq_c);
    }

    /// OXCL arena colliders block movement/raycast: a shot at the baked wall hits.
    #[test]
    fn ecs_room_oxcl_arena_raycast() {
        let arena = engine_physics::oxcl::CookedMesh {
            vertices: vec![
                [-50.0, 0.0, -50.0],
                [50.0, 0.0, -50.0],
                [50.0, 0.0, 50.0],
                [-50.0, 0.0, 50.0],
            ],
            indices: vec![0, 1, 2, 0, 2, 3],
        };
        let mut room = EcsRoomState::new("ecs-ray", Some(&arena));
        // Straight down from above the floor: must hit the arena mesh.
        let hit = room.raycast_hit([0.0, 10.0, 0.0], [0.0, -1.0, 0.0], 100.0);
        assert!(hit.is_some(), "ray down must hit the baked OXCL arena");
        let (_entity, dist) = hit.unwrap();
        assert!((dist - 10.0).abs() < 0.5, "hit at floor level, got {dist}");
    }
}

#[cfg(test)]
mod bench {
    use super::*;
    use std::hint::black_box;
    use std::time::Instant;

    fn rss_mb() -> f64 {
        let s = std::fs::read_to_string("/proc/self/statm").unwrap_or_default();
        let kb: f64 = s.split_whitespace().nth(1).and_then(|p| p.parse().ok()).unwrap_or(0.0);
        kb * 4.0 / 1024.0
    }

    fn pctl(mut v: Vec<f64>, p: f64) -> f64 {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[(v.len() as f64 * p).min((v.len() - 1) as f64) as usize]
    }

    #[test]
    fn bench_ecs_single_thread_capacity() {
        // --- A: 1 room, 1000 players, all moving ---
        let mut room = EcsRoomState::new("bench-1k", None);
        let t0 = Instant::now();
        for i in 0..1000 {
            room.add_player(&format!("s{i}"), i as i64 + 1, (i % 40) as f64 * 15.0, (i / 40) as f64 * 15.0);
        }
        let spawn_s = t0.elapsed().as_secs_f64();
        for i in 0..1000 {
            room.apply_input(&format!("s{i}"), &[input_flags::D]);
        }
        let mut ticks = Vec::new();
        let mut delta_bytes = 0usize;
        for _ in 0..120 {
            let t = Instant::now();
            let d = room.tick(1.0 / 60.0);
            delta_bytes = d.len();
            ticks.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        black_box(&ticks);
        println!("[A] 1 room / 1000 players: spawn {:.0}ms, tick mean {:.3}ms p50 {:.3} p95 {:.3} p99 {:.3}, delta/tick {}B, RSS+{:.1}MB",
            spawn_s * 1000.0,
            ticks.iter().sum::<f64>() / ticks.len() as f64,
            pctl(ticks.clone(), 0.5), pctl(ticks.clone(), 0.95), pctl(ticks.clone(), 0.99),
            delta_bytes, rss_mb());

        // --- B: 100 rooms x 100 players (tick all rooms sequentially per frame) ---
        let mut rooms: Vec<EcsRoomState> = (0..100)
            .map(|r| {
                let mut room = EcsRoomState::new(&format!("bench-{r}"), None);
                for i in 0..100 {
                    room.add_player(&format!("s{i}"), i as i64 + 1, (i % 10) as f64 * 15.0, (i / 10) as f64 * 15.0);
                    room.apply_input(&format!("s{i}"), &[input_flags::D | input_flags::S]);
                }
                room
            })
            .collect();
        let mut frame = Vec::new();
        let mut bytes = 0usize;
        for _ in 0..120 {
            let t = Instant::now();
            for room in &mut rooms {
                bytes += room.tick(1.0 / 60.0).len();
            }
            frame.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        black_box(&frame);
        println!("[B] 100 rooms / 10000 players total: frame mean {:.3}ms p50 {:.3} p95 {:.3} p99 {:.3}, deltas/frame {:.0}KB, RSS+{:.1}MB",
            frame.iter().sum::<f64>() / frame.len() as f64,
            pctl(frame.clone(), 0.5), pctl(frame.clone(), 0.95), pctl(frame.clone(), 0.99),
            bytes as f64 / 120.0 / 1024.0, rss_mb());
    }
}
