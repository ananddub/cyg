//! # Colyseus Schema State Encoder
//!
//! Produces `@colyseus/schema`-compatible wire bytes so real SDK clients can
//! decode room state without any server-side JS:
//!
//! - Root state fields (declared on the client's root schema class):
//!   `players` (MapSchema of a child schema, field index 0) and
//!   `tick` (number, field index 1).
//! - `ROOM_STATE` (14) full snapshots for newly joined clients.
//! - `ROOM_STATE_PATCH` (15) delta patches for live clients.
//!
//! Wire format verified against `@colyseus/schema` `encoder/EncodeOperation.ts`
//! + `decoder/DecodeOperation.ts` (0.18 monorepo fork):
//! - Schema field ops: single byte `(fieldIndex | OPERATION)` — ADD=128,
//!   REPLACE=0, DELETE=64; child schema values are a msgpack refId (the
//!   client falls back to its declared class when no TYPE_ID byte follows).
//! - Map entries: OPERATION byte, msgpack numeric index, on ADD also the
//!   msgpack string key, then the value.
//! - Structures switch via `255` + msgpack refId (refId 0 = root).
//! - msgpack: fixint/uint8..uint32/float64 for numbers, fixstr/str8 for keys.

use std::collections::{HashMap, HashSet};

/// OPERATION.ADD from @colyseus/schema spec.ts.
const OP_ADD: u8 = 128;
/// OPERATION.REPLACE from @colyseus/schema spec.ts.
const OP_REPLACE: u8 = 0;
/// SWITCH_TO_STRUCTURE from @colyseus/schema spec.ts.
const SWITCH: u8 = 255;

/// Field index of `players` on the client's root schema class.
const FIELD_PLAYERS: u8 = 0;
/// Field index of `tick` on the client's root schema class.
const FIELD_TICK: u8 = 1;

#[derive(Debug, Clone, Copy)]
struct PlayerEntry {
    /// Stable numeric index used in map operations.
    map_index: u32,
    /// Stable schema refId of this player instance.
    ref_id: u32,
    x: f64,
    y: f64,
}

/// A removed player pending a DELETE op in the next patch.
#[derive(Debug, Clone, Copy)]
struct RemovedEntry {
    map_index: u32,
}

/// Server-side room state that encodes into @colyseus/schema wire bytes.
#[derive(Debug)]
pub struct ColyseusRoomState {
    players: HashMap<String, PlayerEntry>,
    /// Insertion order for deterministic snapshots.
    order: Vec<String>,
    next_map_index: u32,
    next_ref_id: u32,
    players_map_ref_id: u32,
    tick: u64,
    players_dirty: bool,
    tick_dirty: bool,
    changed_players: HashSet<String>,
    new_players: HashSet<String>,
    removed_players: HashMap<String, RemovedEntry>,
}

impl Default for ColyseusRoomState {
    fn default() -> Self {
        Self::new()
    }
}

impl ColyseusRoomState {
    /// refIds 0 (root) and 1 (players map) are reserved.
    pub fn new() -> Self {
        Self {
            players: HashMap::new(),
            order: Vec::new(),
            next_map_index: 1,
            next_ref_id: 2,
            players_map_ref_id: 1,
            tick: 0,
            players_dirty: false,
            tick_dirty: false,
            changed_players: HashSet::new(),
            new_players: HashSet::new(),
            removed_players: HashMap::new(),
        }
    }

    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// Advance the tick; marks the snapshot dirty for the next patch.
    pub fn advance_tick(&mut self) {
        self.tick += 1;
        self.tick_dirty = true;
    }

    /// Add a new player; emits an ADD on the next patch.
    pub fn add_player(&mut self, key: &str, x: f64, y: f64) {
        let entry = PlayerEntry {
            map_index: self.next_map_index,
            ref_id: self.next_ref_id,
            x,
            y,
        };
        self.next_map_index += 1;
        self.next_ref_id += 1;
        self.players.insert(key.to_string(), entry);
        self.order.push(key.to_string());
        self.players_dirty = true;
        self.new_players.insert(key.to_string());
    }

    /// Update a player's position; emits REPLACEs on the next patch.
    pub fn update_player(&mut self, key: &str, x: f64, y: f64) {
        if let Some(e) = self.players.get_mut(key) {
            e.x = x;
            e.y = y;
            self.players_dirty = true;
            self.changed_players.insert(key.to_string());
        }
    }

    pub fn player_count(&self) -> usize {
        self.players.len()
    }

    /// Remove a player; emits a map DELETE on the next patch.
    pub fn remove_player(&mut self, key: &str) {
        if let Some(entry) = self.players.remove(key) {
            self.order.retain(|k| k != key);
            self.changed_players.remove(key);
            self.new_players.remove(key);
            self.removed_players.insert(
                key.to_string(),
                RemovedEntry {
                    map_index: entry.map_index,
                },
            );
            self.players_dirty = true;
        }
    }

    /// Full snapshot: send as `ROOM_STATE` (14) to newly joined clients.
    pub fn snapshot(&self) -> Vec<u8> {
        self.build_snapshot()
    }

    /// Consuming variant: builds the snapshot and clears dirty flags, so a
    /// subsequent `patch()` only contains post-snapshot changes. Use this
    /// when the state is only ever observed through snapshots+patches.
    pub fn snapshot_consuming(&mut self) -> Vec<u8> {
        let out = self.build_snapshot();
        self.players_dirty = false;
        self.tick_dirty = false;
        self.changed_players.clear();
        self.new_players.clear();
        self.removed_players.clear();
        out
    }

    fn build_snapshot(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(128);

        // Root: players field (index 0, ADD) -> MapSchema instance refId.
        out.push(OP_ADD | FIELD_PLAYERS);
        mp_number(&mut out, self.players_map_ref_id as f64);

        for key in &self.order {
            let entry = &self.players[key];

            // Switch to the players map.
            out.push(SWITCH);
            mp_number(&mut out, self.players_map_ref_id as f64);

            // Map entry ADD: [op][index][key][value refId]
            out.push(OP_ADD);
            mp_number(&mut out, entry.map_index as f64);
            mp_str(&mut out, key);
            mp_number(&mut out, entry.ref_id as f64);

            // Switch to the player instance and emit its fields (ADDs).
            out.push(SWITCH);
            mp_number(&mut out, entry.ref_id as f64);
            out.push(OP_ADD | 0); // x (field index 0)
            mp_number(&mut out, entry.x);
            out.push(OP_ADD | 1); // y (field index 1)
            mp_number(&mut out, entry.y);
        }

        // Back to root, then tick (index 1, ADD).
        out.push(SWITCH);
        mp_number(&mut out, 0.0);
        out.push(OP_ADD | FIELD_TICK);
        mp_number(&mut out, self.tick as f64);

        out
    }

    /// Full snapshot variant that also clears dirty tracking (equivalent to
    /// `snapshot()` followed by consuming the pending patch).
    /// Delta patch: send as `ROOM_STATE_PATCH` (15) to live clients.
    /// Starts from the root ref (each patch decodes from the root).
    pub fn patch(&mut self) -> Vec<u8> {
        let mut out = Vec::with_capacity(64);

        // DELETEs first: switch to the map, emit [OP_DELETE][index] per removal.
        if !self.removed_players.is_empty() {
            out.push(SWITCH);
            mp_number(&mut out, self.players_map_ref_id as f64);
            let mut indices: Vec<u32> =
                self.removed_players.values().map(|r| r.map_index).collect();
            indices.sort_unstable();
            for idx in indices {
                out.push(64); // OPERATION.DELETE
                mp_number(&mut out, idx as f64);
            }
        }

        if self.players_dirty {
            for key in &self.order {
                let entry = &self.players[key];
                let is_new = self.new_players.contains(key);
                let changed = self.changed_players.contains(key);
                if !is_new && !changed {
                    continue;
                }

                if is_new {
                    // Switch to map, ADD entry.
                    out.push(SWITCH);
                    mp_number(&mut out, self.players_map_ref_id as f64);
                    out.push(OP_ADD);
                    mp_number(&mut out, entry.map_index as f64);
                    mp_str(&mut out, key);
                    mp_number(&mut out, entry.ref_id as f64);
                }

                // Switch to player, REPLACE fields.
                out.push(SWITCH);
                mp_number(&mut out, entry.ref_id as f64);
                out.push(OP_REPLACE | 0); // x
                mp_number(&mut out, entry.x);
                out.push(OP_REPLACE | 1); // y
                mp_number(&mut out, entry.y);
            }
            // Back to root for the tick field.
            out.push(SWITCH);
            mp_number(&mut out, 0.0);
        }

        if self.tick_dirty {
            out.push(OP_REPLACE | FIELD_TICK);
            mp_number(&mut out, self.tick as f64);
        }

        self.players_dirty = false;
        self.tick_dirty = false;
        self.changed_players.clear();
        self.new_players.clear();
        out
    }
}

// ---------------------------------------------------------------------------
// msgpack primitives (matching @colyseus/schema encoding/encode.ts)
//
// NOTE: this fork forces LITTLE-endian for all multi-byte numerics
// (see `const _isLittleEndian = true` in encode.ts).
// ---------------------------------------------------------------------------

/// msgpack number: integers via fixint/uint8..uint32/int8.., floats via f64.
pub fn mp_number(out: &mut Vec<u8>, value: f64) {
    if value.is_nan() {
        out.push(0x00);
        return;
    }
    if value.fract() == 0.0 && value.abs() <= 9.007_199_254_740_992e15 {
        let v = value as i64;
        if v >= 0 {
            if v < 0x80 {
                out.push(v as u8);
            } else if v < 0x100 {
                out.push(0xcc);
                out.push(v as u8);
            } else if v < 0x10000 {
                out.push(0xcd);
                out.extend_from_slice(&(v as u16).to_le_bytes());
            } else if v < 0x1_0000_0000 {
                out.push(0xce);
                out.extend_from_slice(&(v as u32).to_le_bytes());
            } else {
                out.push(0xcf);
                out.extend_from_slice(&(v as u64).to_le_bytes());
            }
        } else if v >= -0x20 {
            out.push(0xe0 | ((v + 0x20) as u8));
        } else if v >= -0x80 {
            out.push(0xd0);
            out.push(v as i8 as u8);
        } else if v >= -0x8000 {
            out.push(0xd1);
            out.extend_from_slice(&(v as i16).to_le_bytes());
        } else if v >= -0x8000_0000 {
            out.push(0xd2);
            out.extend_from_slice(&(v as i32).to_le_bytes());
        } else {
            out.push(0xd3);
            out.extend_from_slice(&(v as u64).to_le_bytes());
        }
    } else {
        out.push(0xcb);
        out.extend_from_slice(&value.to_le_bytes());
    }
}

/// msgpack string: fixstr / str8 / str16.
pub fn mp_str(out: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    let len = bytes.len();
    if len < 0x20 {
        out.push(len as u8 | 0xa0);
    } else if len < 0x100 {
        out.push(0xd9);
        out.push(len as u8);
    } else {
        out.push(0xda);
        out.extend_from_slice(&(len as u16).to_be_bytes());
    }
    out.extend_from_slice(bytes);
}
