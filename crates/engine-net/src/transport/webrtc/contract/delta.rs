
use engine_webrtc_protocol::{
    DELTA_HEADER_LEN, MAX_DELTA_BODY, MAX_DELTA_CHUNK, delta_fragment_count, write_delta_frame,
};

use super::FRAME_MAX_WIRE_LEN;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeltaTooLarge {

    BodyTooLarge { body_len: usize, max: usize },
}

impl core::fmt::Display for DeltaTooLarge {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DeltaTooLarge::BodyTooLarge { body_len, max } => write!(
                f,
                "delta body {body_len} B needs more than {max} B of fragments"
            ),
        }
    }
}

impl std::error::Error for DeltaTooLarge {}

pub fn split_delta(
    seq: u16,
    tick: u32,
    body: &[u8],
    out: &mut Vec<Vec<u8>>,
) -> Result<usize, DeltaTooLarge> {
    let Some(total) = delta_fragment_count(body.len()) else {
        out.clear();
        return Err(DeltaTooLarge::BodyTooLarge {
            body_len: body.len(),
            max: MAX_DELTA_BODY,
        });
    };

    out.clear();
    let mut scratch = Vec::with_capacity(MAX_DELTA_CHUNK + DELTA_HEADER_LEN);
    for (idx, chunk) in body.chunks(MAX_DELTA_CHUNK).enumerate() {
        write_delta_frame(&mut scratch, seq, tick, idx as u8, total, chunk);
        debug_assert!(
            scratch.len() <= FRAME_MAX_WIRE_LEN,
            "frame exceeded the MTU"
        );
        out.push(scratch.clone());
    }
    if out.is_empty() {

        write_delta_frame(&mut scratch, seq, tick, 0, 1, &[]);
        out.push(scratch.clone());
    }
    debug_assert_eq!(out.len(), total as usize);
    Ok(out.len())
}
