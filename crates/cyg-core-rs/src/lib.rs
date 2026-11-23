//! cyg-core-rs — the single-source CYG client core.
//!
//! One Rust implementation of everything every client language needs:
//!
//! - schema **descriptor** parsing (reuses `engine-schema`, the same crate the
//!   host builds descriptors with — drift between host and client is
//!   structurally impossible),
//! - **OXS2 snapshot/patch decode** (ported 1:1 from `sdk/client/web/cyg.js`,
//!   the reference decoder — tests pin both against identical bytes),
//! - delta **payload unwrapping** (fragment reassembly via the verified
//!   `engine-webrtc-protocol` codec, msgpack-style prefix strip, envelope
//!   split),
//! - **input encoding** in both lanes (schema-input packing and the mesh
//!   `InputFrame`),
//! - **client-side prediction / rollback** (`predict.rs`): input buffering,
//!   rewind-and-replay reconciliation and divergence/drift telemetry —
//!   design inspired by Colyseus's `@colyseus/sdk` `src/predict/`
//!   (MIT-licensed); reimplemented over OXS2 entities, game-agnostic.
//!
//! Built two ways from the same source:
//! - `wasm32-unknown-unknown --features wasm` → the web SDK core (wasm-bindgen),
//! - default native build → a C-ABI static/dynamic lib (`src/ffi.rs`) that
//!   thin Unity/C#/Kotlin/Swift wrappers bind against.
//!
//! What lives OUTSIDE this crate: transports (sockets/WebSocket) and UI. Each
//! language provides ~100-200 lines of glue; the protocol logic is here only.

pub mod state;
pub use state::StateEvent;
pub mod input;
pub mod interp;
pub mod predict;
pub mod wire;
// Native C-ABI (Mutex slots). On the wasm build wasm_ffi.rs provides the
// same symbols with thread-local slots — both never compile together.
#[cfg(not(feature = "wasm"))]
pub mod ffi;

// Bindgen layer for JS frontends that want object-shaped bindings. The
// default web path uses the plain C-ABI wasm (wasm_ffi) instead.
#[cfg(feature = "web-bindgen")]
pub mod wasm;

// C-ABI exports are available in BOTH builds; on native the Mutex-backed
// slots in ffi.rs serve, on wasm the thread-local slots in wasm_ffi.rs.
#[cfg(feature = "wasm")]
pub mod wasm_ffi;

pub use engine_schema::{
    DESCRIPTOR_VERSION, Descriptor, INPUT_SCHEMA_NAME, RootState, SchemaDef, SchemaField, WireKind,
    emit_descriptor, parse_descriptor, wire_hash,
};

use interp::Interp;
use state::ClientState;

/// High-level client core. Owns the parsed descriptor and the decoded state;
/// the platform binding feeds it raw frames and pulls typed values out.
pub struct ClientCore {
    descriptor: Descriptor,
    /// The schema the room's main collection rows use (matched by hash).
    state: ClientState,
    /// Ack/tick tail of the last applied payload (see `wire::RawFrame`) —
    /// the prediction rollback reads these to know what the server confirmed.
    last_acked_seq: u16,
    last_server_tick: u32,
    /// Remote-entity history: render at `newest_tick - delay`, blend between snapshots.
    interp: Interp,
}

impl ClientCore {
    /// Parse the schema descriptor bytes (`cyg.schema.bin`).
    pub fn new(descriptor_bytes: &[u8]) -> Result<Self, String> {
        if descriptor_bytes.len() >= 5
            && &descriptor_bytes[..4] == b"OXSD"
            && descriptor_bytes[4] != DESCRIPTOR_VERSION
        {
            return Err(format!(
                "unsupported schema descriptor v{}; this client speaks v{DESCRIPTOR_VERSION} — rebuild client and server together",
                descriptor_bytes[4]
            ));
        }
        let descriptor = engine_schema::parse_descriptor(descriptor_bytes)
            .ok_or_else(|| "bad schema descriptor".to_string())?;
        Ok(Self {
            state: ClientState::new(&descriptor),
            descriptor,
            last_acked_seq: 0,
            last_server_tick: 0,
            interp: Interp::new(2),
        })
    }

    pub fn descriptor(&self) -> &Descriptor {
        &self.descriptor
    }

    pub fn state(&self) -> &ClientState {
        &self.state
    }

    /// Mutable state access — prediction applies inputs through this.
    pub fn state_mut(&mut self) -> &mut ClientState {
        &mut self.state
    }

    /// Highest input seq the last applied payload acknowledged
    /// (the prediction rollback ack; 0 = nothing acked yet).
    pub fn last_acked_seq(&self) -> u16 {
        self.last_acked_seq
    }

    /// Server simulation tick carried by the last applied payload.
    pub fn last_server_tick(&self) -> u32 {
        self.last_server_tick
    }

    /// Number of schemas in the parsed descriptor.
    pub fn schema_count(&self) -> usize {
        self.descriptor.schemas.len()
    }

    /// Schema metadata by position (binding glue builds its language mirror
    /// from these — no second descriptor parser lives in the bindings).
    pub fn schema_at(&self, index: usize) -> Option<&SchemaDef> {
        self.descriptor.schemas.get(index)
    }

    /// Number of collections in the room state (descriptor root).
    pub fn collection_count(&self) -> usize {
        self.state.collections.len()
    }

    /// Collection metadata (name + schema hash) by position.
    pub fn collection_at(&self, index: usize) -> Option<&state::Collection> {
        self.state.collections.get(index)
    }

    /// Feed one raw room-data payload (post-transport bytes). Handles fragment
    /// reassembly, prefix strip and envelope split internally; returns the
    /// events the decode produced (empty when the frame was a stale patch or
    /// duplicate fragment).
    pub fn apply_payload(&mut self, payload: &[u8]) -> Result<Vec<StateEvent>, String> {
        let frame = wire::unwrap_payload(payload)?;
        self.last_acked_seq = frame.acked_seq;
        self.last_server_tick = frame.server_tick;
        let events = self.state.apply_section(frame.section, &self.descriptor)?;
        // Every applied section is one moment in time: record it for interpolation. The tick comes
        // from the envelope tail, so the ring and the prediction clock share one notion of time.
        self.interp
            .push(frame.server_tick, &self.state, &self.descriptor);
        Ok(events)
    }

    /// Snapshot history and its sampling rules (client-side interpolation).
    pub fn interp(&self) -> &Interp {
        &self.interp
    }

    /// Mutable interpolation access — changing the delay, reading and resetting stats.
    pub fn interp_mut(&mut self) -> &mut Interp {
        &mut self.interp
    }

    /// The tick a renderer should sample at (`newest_tick - delay_ticks`).
    pub fn render_tick(&self) -> Option<u32> {
        self.interp.render_tick()
    }

    /// Values of `id` at the current render tick, appended to `out`.
    ///
    /// Read-only: interpolation never writes back into [`ClientState`], because prediction compares
    /// the client's own values against the server's and a blended value would read as drift.
    pub fn sample_render(&mut self, id: u32, out: &mut Vec<f32>) -> bool {
        match self.interp.render_tick() {
            Some(t) => self.interp.sample(id, t, out),
            None => false,
        }
    }

    /// Apply a raw OXS2 section directly (test/bypass path).
    pub fn apply_section_direct(
        &mut self,
        section: &[u8],
    ) -> Result<Vec<state::StateEvent>, String> {
        self.state.apply_section(section, &self.descriptor)
    }

    /// Encode a schema-lane input frame: `[seq u16 LE][Input fields in order]`.
    /// `values` must match the `Input` schema's field order.
    pub fn encode_schema_input(
        &self,
        seq: u16,
        values: &[input::InputValue],
    ) -> Result<Vec<u8>, String> {
        let input_schema = self
            .descriptor
            .schemas
            .iter()
            .find(|s| s.name == engine_schema::INPUT_SCHEMA_NAME)
            .ok_or_else(|| "descriptor has no Input schema".to_string())?;
        input::pack_schema_input(seq, values, &input_schema.fields)
    }

    /// Encode the mesh-lane `InputFrame` (`[0x01][seq][flags][aim][move]`).
    pub fn encode_mesh_input(&self, frame: engine_webrtc_protocol::InputFrame) -> Vec<u8> {
        let mut out = Vec::with_capacity(engine_webrtc_protocol::INPUT_WIRE_LEN);
        engine_webrtc_protocol::encode_input(&frame, &mut out);
        out
    }
}
