mod ack;
mod assembler;
mod checksum;
mod delta;
mod evict;
mod fragment;
mod pack;
mod packer;
mod partial;
mod stats;

pub use crate::frames::is_newer;
pub use ack::{ACK_WIRE_LEN, Ack, AckKind, OP_ACK, parse_ack, write_ack};
pub use assembler::FragmentAssembler;
pub use checksum::{
    CHECKSUM_WIRE_LEN, ChecksumReport, OP_CHECKSUM, parse_checksum, write_checksum,
};
pub use delta::{
    DELTA_HEADER_LEN, DeltaHeader, MAX_DELTA_CHUNK, OP_DELTA, delta_fragment_count,
    write_delta_frame,
};
pub use fragment::DeltaFragment;
pub use pack::{DeltaPacking, Fragments, PackError, pack_delta, pack_delta_frames};
pub use packer::DeltaPacker;
pub use partial::AssembledDelta;

/// `frag_total` is a `u8`, and reassembling more chunks than this is how a stuck
/// client turns packet loss into a memory leak.
pub const MAX_FRAGMENTS: usize = 255;

/// The largest delta body the channel layer will carry. Bigger than
/// `engine_wasm::MAX_OUTBOUND_DELTA_SIZE`, so the ABI limit is always the binding one.
pub const MAX_DELTA_BODY: usize = MAX_FRAGMENTS * MAX_DELTA_CHUNK;

/// A partially received delta older than this is a leak, not a message.
pub const FRAGMENT_TTL_MS: u64 = 3000;

/// How many deltas one receiver may hold half-assembled at once: `ordered: false`
/// interleaves their chunks, so one slot would throw away recoverable deltas.
pub const MAX_INFLIGHT_DELTAS: usize = 8;

/// Cap on buffered fragment bodies, so a peer spraying headers cannot make the
/// receiver reserve for deltas it will never finish.
pub const MAX_INFLIGHT_BYTES: usize = 1 << 20;

/// True when a receiver that already holds `newest` must throw `seq` away.
#[inline]
pub fn is_superseded(newest: u16, seq: u16) -> bool {
    !is_newer(newest, seq)
}
