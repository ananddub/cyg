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

use std::collections::HashMap;

use bytes::{BufMut, Bytes, BytesMut};

/// OPERATION.ADD from @colyseus/schema spec.ts.
const OP_ADD: u8 = 128;
/// OPERATION.REPLACE from @colyseus/schema spec.ts.
const OP_REPLACE: u8 = 0;
/// SWITCH_TO_STRUCTURE from @colyseus/schema spec.ts.
const SWITCH: u8 = 255;

/// Player entry was added since the last patch (emit map ADD).
const F_NEW: u8 = 1;
/// Player entry fields changed since the last patch (emit REPLACEs).
const F_CHANGED: u8 = 2;

/// Field index of `players` on the client's root schema class.
const FIELD_PLAYERS: u8 = 0;
/// Field index of `tick` on the client's root schema class.
const FIELD_TICK: u8 = 1;
/// Field index of `coin_x` on the client's root schema class.
const FIELD_COIN_X: u8 = 2;
/// Field index of `coin_y` on the client's root schema class.
const FIELD_COIN_Y: u8 = 3;

#[derive(Debug, Clone, Copy)]
struct PlayerEntry {
    /// Stable numeric index used in map operations.
    map_index: u32,
    /// Stable schema refId of this player instance.
    ref_id: u32,
    x: f64,
    y: f64,
    score: f64,
    /// Dirty bits (`F_NEW`/`F_CHANGED`) — avoids side hashsets and per-tick
    /// string hashing.
    flags: u8,
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
    coin_x: f64,
    coin_y: f64,
    players_dirty: bool,
    tick_dirty: bool,
    coin_dirty: bool,
    /// Map indices pending a DELETE op in the next patch.
    removed_indices: Vec<u32>,
    /// Patch encode scratch; its allocation is reused across patches.
    pb: BytesMut,
    /// Sort scratch for removed indices; reused across patches.
    rm_scratch: Vec<u32>,
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
            coin_x: 0.0,
            coin_y: 0.0,
            players_dirty: false,
            tick_dirty: false,
            coin_dirty: false,
            removed_indices: Vec::new(),
            pb: BytesMut::with_capacity(256),
            rm_scratch: Vec::new(),
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
    pub fn add_player(&mut self, key: &str, x: f64, y: f64, score: f64) {
        let entry = PlayerEntry {
            map_index: self.next_map_index,
            ref_id: self.next_ref_id,
            x,
            y,
            score,
            flags: F_NEW,
        };
        self.next_map_index += 1;
        self.next_ref_id += 1;
        self.players.insert(key.to_string(), entry);
        self.order.push(key.to_string());
        self.players_dirty = true;
    }

    /// Update a player's position and score; emits REPLACEs on the next patch.
    /// Just sets a dirty bit on the entry — no hashing or allocation.
    pub fn update_player(&mut self, key: &str, x: f64, y: f64, score: f64) {
        if let Some(e) = self.players.get_mut(key) {
            e.x = x;
            e.y = y;
            e.score = score;
            e.flags |= F_CHANGED;
            self.players_dirty = true;
        }
    }

    /// Move the shared pickup (coin); emits REPLACEs on the next patch.
    pub fn set_coin(&mut self, x: f64, y: f64) {
        self.coin_x = x;
        self.coin_y = y;
        self.coin_dirty = true;
    }

    pub fn player_count(&self) -> usize {
        self.players.len()
    }

    /// Remove a player; emits a map DELETE on the next patch.
    pub fn remove_player(&mut self, key: &str) {
        if let Some(entry) = self.players.remove(key) {
            self.order.retain(|k| k != key);
            self.removed_indices.push(entry.map_index);
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
        for e in self.players.values_mut() {
            e.flags = 0;
        }
        self.removed_indices.clear();
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
            out.push(OP_ADD | 2); // score (field index 2)
            mp_number(&mut out, entry.score);
        }

        // Back to root: coin fields (ADD), then tick (index 1, ADD).
        out.push(SWITCH);
        mp_number(&mut out, 0.0);
        out.push(OP_ADD | FIELD_COIN_X);
        mp_number(&mut out, self.coin_x);
        out.push(OP_ADD | FIELD_COIN_Y);
        mp_number(&mut out, self.coin_y);
        out.push(OP_ADD | FIELD_TICK);
        mp_number(&mut out, self.tick as f64);

        out
    }

    /// Full snapshot variant that also clears dirty tracking (equivalent to
    /// `snapshot()` followed by consuming the pending patch).
    /// Delta patch: send as `ROOM_STATE_PATCH` (15) to live clients.
    /// Starts from the root ref (each patch decodes from the root).
    /// Encoded into a persistent scratch and returned as a refcounted
    /// [`Bytes`] — the scratch keeps its capacity for the next patch.
    pub fn patch(&mut self) -> Bytes {
        let out = &mut self.pb;
        out.reserve(64);

        // DELETEs first: switch to the map, emit [OP_DELETE][index] per removal.
        if !self.removed_indices.is_empty() {
            out.put_u8(SWITCH);
            mp_number(out, self.players_map_ref_id as f64);
            let mut indices = std::mem::take(&mut self.rm_scratch);
            indices.clear();
            indices.extend(self.removed_indices.drain(..));
            indices.sort_unstable();
            for &idx in &indices {
                out.put_u8(64); // OPERATION.DELETE
                mp_number(out, idx as f64);
            }
            self.rm_scratch = indices;
        }

        if self.players_dirty {
            for key in &self.order {
                let entry = &self.players[key];
                let is_new = entry.flags & F_NEW != 0;
                let changed = entry.flags & F_CHANGED != 0;
                if !is_new && !changed {
                    continue;
                }

                if is_new {
                    // Switch to map, ADD entry.
                    out.put_u8(SWITCH);
                    mp_number(out, self.players_map_ref_id as f64);
                    out.put_u8(OP_ADD);
                    mp_number(out, entry.map_index as f64);
                    mp_str(out, key);
                    mp_number(out, entry.ref_id as f64);
                }

                // Switch to player, REPLACE fields.
                out.put_u8(SWITCH);
                mp_number(out, entry.ref_id as f64);
                out.put_u8(OP_REPLACE | 0); // x
                mp_number(out, entry.x);
                out.put_u8(OP_REPLACE | 1); // y
                mp_number(out, entry.y);
                out.put_u8(OP_REPLACE | 2); // score
                mp_number(out, entry.score);
            }
            // Back to root for coin + tick fields.
            out.put_u8(SWITCH);
            mp_number(out, 0.0);

            for e in self.players.values_mut() {
                e.flags = 0;
            }
        }

        if self.coin_dirty {
            out.put_u8(OP_REPLACE | FIELD_COIN_X);
            mp_number(out, self.coin_x);
            out.put_u8(OP_REPLACE | FIELD_COIN_Y);
            mp_number(out, self.coin_y);
        }

        if self.tick_dirty {
            out.put_u8(OP_REPLACE | FIELD_TICK);
            mp_number(out, self.tick as f64);
        }

        self.players_dirty = false;
        self.tick_dirty = false;
        self.coin_dirty = false;
        out.split().freeze()
    }
}

// ---------------------------------------------------------------------------
// msgpack primitives (matching @colyseus/schema encoding/encode.ts)
//
// NOTE: this fork forces LITTLE-endian for all multi-byte numerics
// (see `const _isLittleEndian = true` in encode.ts).
// ---------------------------------------------------------------------------

/// msgpack number: integers via fixint/uint8..uint32/int8.., floats via f64.
pub fn mp_number(out: &mut impl BufMut, value: f64) {
    if value.is_nan() {
        out.put_u8(0x00);
        return;
    }
    if value.fract() == 0.0 && value.abs() <= 9.007_199_254_740_992e15 {
        let v = value as i64;
        if v >= 0 {
            if v < 0x80 {
                out.put_u8(v as u8);
            } else if v < 0x100 {
                out.put_u8(0xcc);
                out.put_u8(v as u8);
            } else if v < 0x10000 {
                out.put_u8(0xcd);
                out.put_slice(&(v as u16).to_le_bytes());
            } else if v < 0x1_0000_0000 {
                out.put_u8(0xce);
                out.put_slice(&(v as u32).to_le_bytes());
            } else {
                out.put_u8(0xcf);
                out.put_slice(&(v as u64).to_le_bytes());
            }
        } else if v >= -0x20 {
            out.put_u8(0xe0 | ((v + 0x20) as u8));
        } else if v >= -0x80 {
            out.put_u8(0xd0);
            out.put_u8(v as i8 as u8);
        } else if v >= -0x8000 {
            out.put_u8(0xd1);
            out.put_slice(&(v as i16).to_le_bytes());
        } else if v >= -0x8000_0000 {
            out.put_u8(0xd2);
            out.put_slice(&(v as i32).to_le_bytes());
        } else {
            out.put_u8(0xd3);
            out.put_slice(&(v as u64).to_le_bytes());
        }
    } else {
        out.put_u8(0xcb);
        out.put_slice(&value.to_le_bytes());
    }
}

/// msgpack string: fixstr / str8 / str16.
pub fn mp_str(out: &mut impl BufMut, s: &str) {
    let bytes = s.as_bytes();
    let len = bytes.len();
    if len < 0x20 {
        out.put_u8(len as u8 | 0xa0);
    } else if len < 0x100 {
        out.put_u8(0xd9);
        out.put_u8(len as u8);
    } else {
        out.put_u8(0xda);
        out.put_slice(&(len as u16).to_be_bytes());
    }
    out.put_slice(bytes);
}
