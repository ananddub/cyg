mod read;
mod reader;
mod write;

pub use read::{Descriptor, parse_descriptor};
pub use write::emit_descriptor;

pub(crate) use reader::Reader;

pub const DESCRIPTOR_MAGIC: &[u8; 4] = b"OXSD";

/// Latest descriptor version this build can write. Version history:
///
/// - `1` name/kind only
/// - `2` root state with scalars + collections
/// - `3` per-field flags and array element kind
/// - `4` per-field wire encoding (bit width + float range)
///
/// A reader accepts anything up to this number, so a v3 descriptor from an older build still
/// loads — its fields simply carry their default encoding.
pub const DESCRIPTOR_VERSION: u8 = 4;
