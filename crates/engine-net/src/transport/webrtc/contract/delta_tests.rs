//! Splitting one delta body into MTU-safe stamped frames.

use engine_webrtc_protocol::{
    DELTA_HEADER_LEN, FragmentAssembler, MAX_DELTA_BODY, MAX_DELTA_CHUNK, MAX_FRAGMENTS, MAX_FRAME,
    DeltaHeader,
};

use super::{DeltaTooLarge, split_delta};

#[test]
fn one_byte_becomes_one_unfragmented_frame() {
    let mut frames = Vec::new();
    assert_eq!(split_delta(7, 42, &[0xAB], &mut frames).unwrap(), 1);
    assert_eq!(frames.len(), 1);
    assert!(frames[0].len() <= MAX_FRAME);

    let (header, chunk) = DeltaHeader::parse(&frames[0]).unwrap();
    assert_eq!(header.seq, 7);
    assert_eq!(header.tick, 42);
    assert_eq!((header.frag_idx, header.frag_total), (0, 1));
    assert!(header.is_single());
    assert_eq!(chunk, &[0xAB]);
}

/// An empty body is still one frame: the receiver must be able to tell
/// "nothing changed this tick" from "the delta never arrived".
#[test]
fn empty_body_is_still_one_stamped_frame() {
    let mut frames = Vec::new();
    assert_eq!(split_delta(9, 10, &[], &mut frames).unwrap(), 1);
    let (header, chunk) = DeltaHeader::parse(&frames[0]).unwrap();
    assert_eq!((header.frag_idx, header.frag_total), (0, 1));
    assert!(chunk.is_empty());
}

#[test]
fn exactly_one_chunk_is_not_fragmented() {
    let body = vec![0x5au8; MAX_DELTA_CHUNK];
    let mut frames = Vec::new();
    assert_eq!(split_delta(11, 3, &body, &mut frames).unwrap(), 1);
    assert_eq!(frames[0].len(), DELTA_HEADER_LEN + MAX_DELTA_CHUNK);
    assert_eq!(frames[0].len(), MAX_FRAME, "one chunk fills the MTU exactly");
    let (header, chunk) = DeltaHeader::parse(&frames[0]).unwrap();
    assert_eq!(header.frag_total, 1);
    assert_eq!(chunk, body.as_slice());
}

#[test]
fn one_byte_over_the_boundary_fragments_twice() {
    let body = vec![0x5au8; MAX_DELTA_CHUNK + 1];
    let mut frames = Vec::new();
    assert_eq!(split_delta(12, 3, &body, &mut frames).unwrap(), 2);
    for (idx, f) in frames.iter().enumerate() {
        let (header, _) = DeltaHeader::parse(f).unwrap();
        assert_eq!(header.frag_idx as usize, idx);
        assert_eq!(header.frag_total, 2);
        assert_eq!(header.seq, 12);
    }
}

/// The largest body the contract carries: 255 chunks, all indices distinct,
/// every frame MTU-safe.
#[test]
fn two_hundred_fifty_five_chunks_is_the_last_legal_delta() {
    let body = vec![0x33u8; MAX_DELTA_BODY];
    let mut frames = Vec::new();
    assert_eq!(split_delta(200, 7, &body, &mut frames).unwrap(), MAX_FRAGMENTS);
    assert_eq!(frames.len(), 255);
    let mut seen = vec![false; 255];
    for f in &frames {
        let (header, chunk) = DeltaHeader::parse(f).unwrap();
        assert_eq!(header.frag_total, 255);
        assert_eq!(header.seq, 200);
        assert!(seen[header.frag_idx as usize].eq(&false), "frag_idx reused");
        seen[header.frag_idx as usize] = true;
        assert!(f.len() <= MAX_FRAME);
        assert!(chunk.len() <= MAX_DELTA_CHUNK);
    }
    assert!(seen.iter().all(|&s| s));
}

#[test]
fn two_hundred_fifty_six_chunks_is_refused_not_truncated() {
    let body = vec![0x33u8; MAX_DELTA_BODY + 1];
    let mut frames = Vec::new();
    assert_eq!(
        split_delta(1, 1, &body, &mut frames),
        Err(DeltaTooLarge::BodyTooLarge {
            body_len: MAX_DELTA_BODY + 1,
            max: MAX_DELTA_BODY,
        })
    );
    assert!(frames.is_empty(), "a refused delta must not leave partial frames");
}

/// The headline MTU guarantee: a delta far bigger than one packet only ever
/// produces frames that fit in one packet, and they reassemble byte-exact.
#[test]
fn an_oversized_delta_produces_only_mtu_safe_frames() {
    let body: Vec<u8> = (0..12_000u32).map(|i| (i % 251) as u8).collect();
    assert!(body.len() > MAX_FRAME);

    let mut frames = Vec::new();
    let count = split_delta(640, 99, &body, &mut frames).unwrap() as u8;
    for f in &frames {
        assert!(f.len() <= MAX_FRAME, "a {} B frame exceeds the MTU", f.len());
        let (header, _) = DeltaHeader::parse(f).unwrap();
        assert_eq!(header.frag_total, count);
        assert_eq!(header.tick, 99);
    }

    // And the other half of the contract: the receiver can put it back.
    let mut assembler = FragmentAssembler::default();
    let mut out = None;
    for f in &frames {
        let (header, chunk) = DeltaHeader::parse(f).unwrap();
        if let Some(done) = assembler.insert(&header, chunk, 0) {
            out = Some(done.body);
        }
    }
    assert_eq!(out.as_deref(), Some(body.as_slice()));
    assert_eq!(frames.len(), 11, "12000 B of body is 11 chunks of 1191");
}
