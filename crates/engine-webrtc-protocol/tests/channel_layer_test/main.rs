mod assembler;
mod buffer;
mod packer;
mod ttl;
mod vectors;

use engine_webrtc_protocol::channel::{DeltaHeader, FragmentAssembler, write_delta_frame};

pub(crate) fn feed(assembler: &mut FragmentAssembler, wire: &[u8], now_ms: u64) -> Option<Vec<u8>> {
    let (header, chunk) = DeltaHeader::parse(wire)?;
    assembler.insert(&header, chunk, now_ms).map(|d| d.body)
}

pub(crate) fn frame(seq: u16, tick: u32, idx: u8, total: u8, chunk: &[u8]) -> Vec<u8> {
    let mut wire = Vec::new();
    write_delta_frame(&mut wire, seq, tick, idx, total, chunk);
    wire
}
