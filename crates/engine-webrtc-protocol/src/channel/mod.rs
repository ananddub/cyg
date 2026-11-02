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

pub const MAX_FRAGMENTS: usize = 255;

pub const MAX_DELTA_BODY: usize = MAX_FRAGMENTS * MAX_DELTA_CHUNK;

pub const FRAGMENT_TTL_MS: u64 = 3000;

pub const MAX_INFLIGHT_DELTAS: usize = 8;

pub const MAX_INFLIGHT_BYTES: usize = 1 << 20;

#[inline]
pub fn is_superseded(newest: u16, seq: u16) -> bool {
    !is_newer(newest, seq)
}
