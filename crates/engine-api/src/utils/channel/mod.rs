pub(crate) mod cursor;
pub(crate) mod parse;
pub(crate) mod stream;

pub use parse::stream_of_channel;
pub use stream::{Stream, channel_of_stream};

pub(crate) use cursor::{decode_cursor, encode_cursor};

pub const MODE_ROOM: i16 = 2;
pub const MODE_GROUP: i16 = 3;
pub const MODE_DM: i16 = 4;

pub(crate) const BAD_CURSOR: &str = "Cursor is invalid or expired.";
pub(crate) const LIMIT_RANGE: &str = "Invalid limit - limit must be between 1 and 100.";
