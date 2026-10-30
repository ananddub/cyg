//! The room module a hosted game carries into its server process.
//!
//! The console stores module versions in `room_modules` and the blob store; the
//! control plane fetches the active version once at boot and hands the bytes to
//! the host loop, which prefers them over the project folder's compiled
//! `room_logic.wasm`. A local-only project (no database) boots with `None` and
//! keeps using the file, exactly as it always has.

#[derive(Clone, Debug)]
pub struct BootModule {
    /// `room_modules.version` the console marked active.
    pub version: i32,
    /// SHA-256 hex the console recorded for these bytes.
    pub sha256: String,
    /// The version's `.wasm` binary, as the blob store holds it.
    pub bytes: Vec<u8>,
}
