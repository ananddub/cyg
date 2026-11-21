//! Client-side interpolation for remote entities.
//!
//! The server sends state at its own rate; a client renders faster than that. Drawing the newest
//! snapshot as it arrives makes every remote entity move in steps. This keeps a short history of
//! snapshots and answers "where was entity N at render tick T" by blending the two snapshots that
//! bracket T — the standard fix (Gaffer On Games, *Snapshot Interpolation*; Gambetta, *Entity
//! Interpolation*).
//!
//! Three rules that matter more than the blend itself:
//!
//! - **Render delay.** T is the newest tick minus `delay_ticks`, so the client is always rendering
//!   the past and the next snapshot usually arrives in time to be blended against.
//! - **Snap, never blend, the first time.** An entity with no earlier snapshot has nothing to
//!   blend from; blending would slide it in from a position it never had.
//! - **Despawn is immediate.** A removed entity must leave the ring at once, or the render delay
//!   turns it into a ghost for the length of the delay.
//!
//! Interpolation is read-only: it never writes back into [`ClientState`], because prediction
//! compares the client's own values against the server's and a blended value would read as drift.

use std::collections::BTreeMap;

use engine_schema::{Descriptor, SchemaDef, WireKind};

use crate::state::{ClientState, Value};

/// Ticks of history kept beyond the render delay.
const HISTORY_TICKS: usize = 32;

/// Per-field values of one tick: `f32::NAN` marks a field that is not numeric (string, array) and
/// therefore cannot be blended.
struct Snapshot {
    tick: u32,
    rows: BTreeMap<u32, Vec<f32>>,
}

/// What the sampler did, and what that says about smoothness.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct InterpStats {
    /// Snapshot ticks kept so far (a rough uptime counter).
    pub snapshots: u64,
    /// Blends between two snapshots.
    pub blended: u64,
    /// Samples answered from a single snapshot: the first sighting of an entity, or a tick with
    /// no earlier bracket. Every one of these is a visible pop.
    pub snapped: u64,
    /// Samples clamped to the oldest snapshot (the client asked for a tick too far back).
    pub clamped: u64,
    /// Samples held at the newest snapshot (nothing newer has arrived yet).
    pub held: u64,
    /// Render-space steps larger than [`Interp::jump_threshold`] between consecutive samples of
    /// the same field. Non-zero means something still teleports.
    pub jumps: u64,
    /// The largest such step seen, in field units.
    pub max_step: f32,
    /// Entities dropped from the ring because the server removed them.
    pub despawns: u64,
}

/// Snapshot history plus the sampling rules.
pub struct Interp {
    delay_ticks: u32,
    jump_threshold: f32,
    /// Oldest first, newest last.
    ticks: std::collections::VecDeque<Snapshot>,
    /// Last value handed to the renderer, per entity, for jump measurement.
    last_out: BTreeMap<u32, Vec<f32>>,
    stats: InterpStats,
}

impl Interp {
    /// Two ticks of delay is the usual default: enough to cover one lost snapshot at 60 Hz edges
    /// without making the world feel a full frame late.
    pub fn new(delay_ticks: u32) -> Self {
        Self {
            delay_ticks,
            jump_threshold: 2.0,
            ticks: std::collections::VecDeque::with_capacity(HISTORY_TICKS),
            last_out: BTreeMap::new(),
            stats: InterpStats::default(),
        }
    }

    pub fn delay_ticks(&self) -> u32 {
        self.delay_ticks
    }

    /// Changes the render delay. Frames already buffered stay; only sampling moves.
    pub fn set_delay_ticks(&mut self, ticks: u32) {
        self.delay_ticks = ticks;
    }

    pub fn set_jump_threshold(&mut self, units: f32) {
        self.jump_threshold = units.max(0.0);
    }

    pub fn stats(&self) -> InterpStats {
        self.stats
    }

    /// Newest tick in the ring, i.e. the last one the server sent.
    pub fn newest_tick(&self) -> Option<u32> {
        self.ticks.back().map(|s| s.tick)
    }

    /// The tick a renderer should ask about right now.
    pub fn render_tick(&self) -> Option<u32> {
        self.newest_tick().map(|t| t.wrapping_sub(self.delay_ticks))
    }

    /// Records the state as of `tick`.
    ///
    /// A repeated tick replaces its own snapshot instead of appending: two sections for the same
    /// tick are one moment in time, and a duplicate would break the bracket search.
    pub fn push(&mut self, tick: u32, state: &ClientState, descriptor: &Descriptor) {
        let rows = collect_rows(state, descriptor);
        self.stats.snapshots += 1;

        if let Some(last) = self.ticks.back() {
            if last.tick == tick {
                self.ticks.back_mut().unwrap().rows = rows;
                return;
            }
            // A tick older than the newest one arrived late (reordered datagram). Drop it: the
            // bracket search assumes ascending ticks, and one stale tick is not worth a sort.
            if tick.wrapping_sub(last.tick) > 0x8000_0000 {
                return;
            }
        }

        self.ticks.push_back(Snapshot { tick, rows });
        while self.ticks.len() > HISTORY_TICKS {
            self.ticks.pop_front();
        }

        // Entities the newest snapshot no longer contains were removed by the server. Erasing them
        // from *every* buffered tick is what keeps the render delay from showing ghosts: the
        // renderer is still looking at ticks that are older than the newest one.
        let newest: std::collections::BTreeSet<u32> = self
            .ticks
            .back()
            .map(|s| s.rows.keys().copied().collect())
            .unwrap_or_default();
        // Count what is about to disappear: every id the ring knows that the newest tick does not.
        let mut known: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
        for snap in self.ticks.iter() {
            known.extend(snap.rows.keys().copied());
        }
        self.stats.despawns += known.iter().filter(|id| !newest.contains(*id)).count() as u64;

        for snap in self.ticks.iter_mut() {
            snap.rows.retain(|id, _| newest.contains(id));
        }
        self.last_out.retain(|id, _| {
            let keep = newest.contains(id);
            if !keep {
                self.stats.despawns += 1;
            }
            keep
        });
    }

    /// Every entity in the newest snapshot, for a renderer that wants to draw them all.
    pub fn entities(&self) -> Vec<u32> {
        self.ticks
            .back()
            .map(|s| s.rows.keys().copied().collect())
            .unwrap_or_default()
    }

    /// Samples `field` of entity `id` at render tick `t`, appending the values to `out`.
    ///
    /// Returns `false` when the entity is not in the ring at all, in which case `out` is left
    /// untouched. A field that is not numeric yields `f32::NAN`.
    pub fn sample(&mut self, id: u32, t: u32, out: &mut Vec<f32>) -> bool {
        let Some((older, newer, blend)) = self.bracket(t) else {
            return false;
        };
        let (Some(a), Some(b)) = (older.rows.get(&id), newer.rows.get(&id)) else {
            return false;
        };
        if a.len() != b.len() {
            return false;
        }

        out.clear();
        out.reserve(a.len());
        for (x, y) in a.iter().zip(b.iter()) {
            let v = if x.is_nan() || y.is_nan() {
                // Non-numeric fields do not blend; take the newer value.
                *y
            } else {
                x + (y - x) * blend
            };
            out.push(v);
        }

        // Jump accounting: a step that big means the blend did not actually smooth anything.
        if let Some(prev) = self.last_out.get(&id) {
            if prev.len() == out.len() {
                for (before, now) in prev.iter().zip(out.iter()) {
                    if before.is_nan() || now.is_nan() {
                        continue;
                    }
                    let step = (now - before).abs();
                    if step > self.jump_threshold {
                        self.stats.jumps += 1;
                        self.stats.max_step = self.stats.max_step.max(step);
                    }
                }
            }
        }
        self.last_out.insert(id, out.clone());
        true
    }

    /// Finds the two snapshots bracketing `t` and how far between them `t` falls.
    ///
    /// `blend == 0.0` means "use `older` as it is" — the caller's single-snapshot and clamp cases
    /// both land here, and the stats record which one it was.
    fn bracket(&mut self, t: u32) -> Option<(&Snapshot, &Snapshot, f32)> {
        let first = self.ticks.front()?;
        let last = self.ticks.back()?;

        if is_after(first.tick, t) {
            // Older than anything we kept.
            self.stats.clamped += 1;
            return Some((first, first, 0.0));
        }
        if is_after(t, last.tick) {
            // Newer than the newest snapshot: hold it rather than extrapolate.
            self.stats.held += 1;
            return Some((last, last, 0.0));
        }

        let mut older = first;
        let mut newer = last;
        for snap in self.ticks.iter() {
            if is_after(snap.tick, t) {
                newer = snap;
                break;
            }
            older = snap;
        }

        if older.tick == newer.tick {
            self.stats.snapped += 1;
            return Some((older, newer, 0.0));
        }
        let span = newer.tick.wrapping_sub(older.tick) as f32;
        let at = t.wrapping_sub(older.tick) as f32;
        self.stats.blended += 1;
        Some((older, newer, (at / span).clamp(0.0, 1.0)))
    }
}

/// `a` comes after `b` in tick order, tolerating the u32 wrap.
fn is_after(a: u32, b: u32) -> bool {
    a != b && a.wrapping_sub(b) < 0x8000_0000
}

/// Flattens a collection row into `f32`, keeping `f32::NAN` in the slots that are not numeric.
fn collect_rows(state: &ClientState, descriptor: &Descriptor) -> BTreeMap<u32, Vec<f32>> {
    let mut rows = BTreeMap::new();
    let Some(collection) = state.collections.first() else {
        return rows;
    };
    let schema = descriptor
        .schemas
        .iter()
        .find(|s| engine_schema::wire_hash(s) == collection.schema_hash);
    let Some(schema) = schema else {
        return rows;
    };
    let numeric: Vec<bool> = schema.fields.iter().map(|f| is_numeric(f.kind)).collect();

    for (&id, entity) in &collection.entities {
        let mut flat = Vec::with_capacity(schema.fields.len());
        for (index, value) in entity.values.iter().enumerate() {
            let blendable = numeric.get(index).copied().unwrap_or(false);
            flat.push(match value {
                Value::Bool(b) if blendable => {
                    if *b {
                        1.0
                    } else {
                        0.0
                    }
                }
                v if blendable => v.as_f64() as f32,
                _ => f32::NAN,
            });
        }
        rows.insert(id, flat);
    }
    rows
}

fn is_numeric(kind: WireKind) -> bool {
    !matches!(kind, WireKind::String | WireKind::Array)
}

/// Field count of the room's main collection, so a binding can size its output buffer once.
pub fn main_field_count(state: &ClientState, descriptor: &Descriptor) -> usize {
    state
        .collections
        .first()
        .and_then(|c| {
            descriptor
                .schemas
                .iter()
                .find(|s| engine_schema::wire_hash(s) == c.schema_hash)
        })
        .map_or(0, |s: &SchemaDef| s.fields.len())
}

#[cfg(test)]
mod tests;
