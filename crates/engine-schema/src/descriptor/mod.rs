mod read;
mod reader;
mod write;

pub use read::{Descriptor, parse_descriptor};
pub use write::emit_descriptor;

pub(crate) use reader::Reader;

pub const DESCRIPTOR_MAGIC: &[u8; 4] = b"OXSD";

/// The one descriptor version this build writes and reads. Version history:
///
/// - `1` name/kind only
/// - `2` root state with scalars + collections
/// - `3` per-field flags and array element kind
/// - `4` per-field wire encoding (bit width + float range)
/// - `5` flattened nested fields + nested child collections
/// - `6` canonical packed OXS2 guest sections
///
/// Older and newer versions are rejected. There is no dual parser and no
/// silent fallback: a mismatch is a "rebuild client and server together" error.
pub const DESCRIPTOR_VERSION: u8 = 6;
