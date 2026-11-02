mod read;
mod reader;
mod write;

pub use read::{parse_descriptor, Descriptor};
pub use write::emit_descriptor;

pub(crate) use reader::Reader;

pub const DESCRIPTOR_MAGIC: &[u8; 4] = b"OXSD";

pub const DESCRIPTOR_VERSION: u8 = 3;
