//! Channel-contract policy for the browser `game-*` DataChannels.
//!
//! [`engine_webrtc_protocol::channel`] owns the *bytes* (header layout,
//! fragment sizing, ack codec). This module owns the *rules* the transport has
//! to apply on top of them: which channel is which lane, what is allowed to
//! arrive on it, how a delta becomes MTU-safe frames, and what happens when a
//! lane's send buffer is full.
//!
//! Everything here is pure — no `Rtc`, no sockets — so the contract is testable
//! without driving a DTLS handshake.

use std::collections::VecDeque;

use engine_webrtc_protocol::{
    HEADER_LEN, INPUT_WIRE_LEN, MAX_DELTA_BODY, MAX_FRAGMENTS, MAX_FRAME, OP_DELTA, OP_EVENT,
    OP_INPUT, delta_fragment_count, is_newer, parse_ack, write_delta_frame, Ack, EventFrame,
};
use str0m::channel::{ChannelConfig, Reliability};

/// The unreliable lane's channel label, matched exactly.
pub const LABEL_UNRELIABLE: &str = "game-unreliable";

/// The reliable lane's channel label, matched exactly.
pub const LABEL_RELIABLE: &str = "game-reliable";

/// Bytes of input-frame body after `[op][seq u16]`.
pub const INPUT_PAYLOAD_LEN: usize = INPUT_WIRE_LEN - HEADER_LEN;

/// A frame is only ever one DataChannel message, and one DataChannel message is
/// only useful if it fits the path MTU: SCTP fragmentation loses the *whole*
/// message when any chunk is lost and keeps reassembly state until it times out.
pub const FRAME_MAX_WIRE_LEN: usize = MAX_FRAME;

/// Frames a lane will hold for a client that is not draining. Sized so the
/// largest legal delta (`MAX_FRAGMENTS` chunks) always fits on its own, which
/// is what makes "drop the oldest whole delta" a rule we can actually keep.
pub const MAX_QUEUED_FRAMES: usize = MAX_FRAGMENTS + 1;

/// Which side of the reliability contract a channel serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lane {
    /// `{ ordered: false, maxRetransmits: 0 }`: inputs, and the server's deltas.
    Unreliable,
    /// `{ ordered: true }`: events and the acks that confirm them.
    Reliable,
}

impl Lane {
    /// The label this lane must carry for a channel to be part of the contract.
    pub fn label(self) -> &'static str {
        match self {
            Lane::Unreliable => LABEL_UNRELIABLE,
            Lane::Reliable => LABEL_RELIABLE,
        }
    }
}

/// What the transport decided about a channel label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelVerdict {
    /// Exact contract label.
    Contract(Lane),
    /// A lane-naming label from an older SDK bundle (for example a prefixed
    /// `client-unreliable`). Accepted, but warned about once, because it is not
    /// something the server should be relying on.
    LegacyAlias(Lane),
    /// Not part of the contract. Deliberately *not* defaulted to reliable: an
    /// unknown channel that silently joins the event lane is how a debug or
    /// telemetry channel ends up carrying gameplay.
    Ignored,
}

/// Match a channel label against the contract.
///
/// The exact labels win; anything else is only accepted if it *ends* with a
/// lane word as its own dash/underscore-separated token. Note that
/// `"unreliable"` ends with `"reliable"`, so the unreliable test must run first
/// — the same ordering trap that made the old `label.contains(...)` chain
/// fragile.
pub fn classify_label(label: &str) -> LabelVerdict {
    match label {
        LABEL_UNRELIABLE => return LabelVerdict::Contract(Lane::Unreliable),
        LABEL_RELIABLE => return LabelVerdict::Contract(Lane::Reliable),
        _ => {}
    }
    if ends_with_token(label, "unreliable") {
        LabelVerdict::LegacyAlias(Lane::Unreliable)
    } else if ends_with_token(label, "reliable") {
        LabelVerdict::LegacyAlias(Lane::Reliable)
    } else {
        LabelVerdict::Ignored
    }
}

fn ends_with_token(label: &str, token: &str) -> bool {
    if label == token {
        return true;
    }
    // The lane word must be its own dash/underscore-separated token at the end:
    // `client-reliable` counts, `fooreliable` and `x-reliable-y` do not.
    match label.strip_suffix(token) {
        Some(rest) => matches!(rest.chars().next_back(), Some('-') | Some('_')),
        None => false,
    }
}

/// Does the SCTP parameters the peer actually negotiated match the lane?
///
/// A label only names a lane; `ordered`/`reliability` are what the channel
/// *is*. Returning the mismatch instead of guessing lets the transport keep
/// serving an older bundle while still reporting that the reliability mapping
/// for that session is not what the contract promises.
pub fn contract_mismatch(lane: Lane, config: &ChannelConfig) -> Option<&'static str> {
    match lane {
        Lane::Unreliable => {
            if config.ordered {
                return Some("game-unreliable channel was opened ordered");
            }
            if !matches!(config.reliability, Reliability::MaxRetransmits { retransmits: 0 }) {
                return Some("game-unreliable channel allows retransmits");
            }
        }
        Lane::Reliable => {
            if !config.ordered {
                return Some("game-reliable channel was opened unordered");
            }
            if !matches!(config.reliability, Reliability::Reliable) {
                return Some("game-reliable channel can drop messages");
            }
        }
    }
    None
}

/// Why a frame was refused rather than handed to the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectReason {
    /// The channel it arrived on is not part of the contract.
    OffContract,
    /// A frame whose opcode belongs to the other lane. Reliability is a
    /// property of the lane, so an ack that arrives unreliated is not an ack.
    WrongLane {
        expected: Lane,
        got: Lane,
    },
    /// The opcode is known but the length is not one legal wire frame.
    Malformed {
        op: u8,
        got: usize,
        want: usize,
    },
}

/// A client frame classified against its lane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inbound {
    /// 50 Hz–128 Hz movement input. `resupplied` means the high-water mark had
    /// already passed this seq: the host may still want it for rollback replay,
    /// so it is flagged rather than thrown away.
    Input {
        seq: u16,
        payload: [u8; INPUT_PAYLOAD_LEN],
        resupplied: bool,
    },
    /// A gameplay event on the ordered lane. The server stamps its own seq when
    /// it fans this out, so the raw frame is passed through untouched.
    Event { raw: Vec<u8> },
    /// Proof the client holds a reliable frame.
    Ack(Ack),
    /// A frame the transport understands the lane for but does not model.
    Other { op: u8, len: usize },
    /// Refused, with the reason counted by the caller.
    Refused(RejectReason),
}

/// Decode an `InputFrame` body: the 20 bytes after `[op][seq u16]`, exactly as
/// the host's `apply_input` expects them.
pub fn input_body(data: &[u8]) -> Option<[u8; INPUT_PAYLOAD_LEN]> {
    let rest = data.get(HEADER_LEN..)?;
    rest.get(..INPUT_PAYLOAD_LEN)
        .map(|body| body.try_into().expect("slice of fixed length"))
}

/// Classify a frame that arrived on `lane` (`None` = a channel outside the
/// contract).
///
/// Lane enforcement is what makes the reliability mapping explicit: a 4-byte
/// ack pattern on the unreliable lane is refused instead of being believed,
/// because SCTP could have dropped it and the server would then believe an
/// event was delivered that never arrived.
pub fn classify_inbound(lane: Option<Lane>, data: &[u8]) -> Inbound {
    let Some(lane) = lane else {
        return Inbound::Refused(RejectReason::OffContract);
    };
    let Some(&op) = data.first() else {
        return Inbound::Refused(RejectReason::Malformed {
            op: 0,
            got: 0,
            want: INPUT_WIRE_LEN,
        });
    };

    match op {
        OP_INPUT => {
            if data.len() != INPUT_WIRE_LEN {
                return Inbound::Refused(RejectReason::Malformed {
                    op,
                    got: data.len(),
                    want: INPUT_WIRE_LEN,
                });
            }
            if lane != Lane::Unreliable {
                return Inbound::Refused(RejectReason::WrongLane {
                    expected: Lane::Unreliable,
                    got: lane,
                });
            }
            let seq = u16::from_le_bytes([data[1], data[2]]);
            match input_body(data) {
                Some(payload) => Inbound::Input {
                    seq,
                    payload,
                    resupplied: false,
                },
                None => Inbound::Refused(RejectReason::Malformed {
                    op,
                    got: data.len(),
                    want: INPUT_WIRE_LEN,
                }),
            }
        }
        engine_webrtc_protocol::OP_ACK => {
            match parse_ack(data) {
                Some(ack) => {
                    if lane != Lane::Reliable {
                        Inbound::Refused(RejectReason::WrongLane {
                            expected: Lane::Reliable,
                            got: lane,
                        })
                    } else {
                        Inbound::Ack(ack)
                    }
                }
                None => Inbound::Refused(RejectReason::Malformed {
                    op,
                    got: data.len(),
                    want: engine_webrtc_protocol::ACK_WIRE_LEN,
                }),
            }
        }
        OP_EVENT => {
            if lane != Lane::Reliable {
                return Inbound::Refused(RejectReason::WrongLane {
                    expected: Lane::Reliable,
                    got: lane,
                });
            }
            match EventFrame::decode(data) {
                Ok(_) => Inbound::Event {
                    raw: data.to_vec(),
                },
                Err(_) => Inbound::Refused(RejectReason::Malformed {
                    op,
                    got: data.len(),
                    want: engine_webrtc_protocol::EVENT_WIRE_LEN,
                }),
            }
        }
        // A client echoing a server delta back is never valid input, and on the
        // unreliable lane it is indistinguishable from a hostile or buggy peer.
        OP_DELTA => Inbound::Refused(RejectReason::Malformed {
            op,
            got: data.len(),
            want: INPUT_WIRE_LEN,
        }),
        other => Inbound::Other {
            op: other,
            len: data.len(),
        },
    }
}

/// Why a delta body could not be sent at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeltaTooLarge {
    /// The body needs more than [`MAX_FRAGMENTS`] chunks. Refused whole: a
    /// truncated delta is a corrupt world state, and an oversized frame is a
    /// message SCTP will reassemble badly or drop entirely.
    BodyTooLarge {
        body_len: usize,
        max: usize,
    },
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

/// Stamp and split one delta into wire frames.
///
/// `out` is cleared and reused so a per-tick broadcast loop does not allocate
/// the outer `Vec` again. Every frame is `<= MAX_FRAME` bytes, carries the same
/// `seq`/`tick` and a correct `frag_idx`/`frag_total`.
pub fn split_delta(
    seq: u16,
    tick: u32,
    body: &[u8],
    out: &mut Vec<Vec<u8>>,
) -> Result<usize, DeltaTooLarge> {
    let total = match delta_fragment_count(body.len()) {
        Some(total) => total,
        None => {
            out.clear();
            return Err(DeltaTooLarge::BodyTooLarge {
                body_len: body.len(),
                max: MAX_DELTA_BODY,
            });
        }
    };

    out.clear();
    let chunk_len = engine_webrtc_protocol::MAX_DELTA_CHUNK;
    let mut scratch = Vec::with_capacity(chunk_len + engine_webrtc_protocol::DELTA_HEADER_LEN);
    for (idx, chunk) in body.chunks(chunk_len).enumerate() {
        write_delta_frame(&mut scratch, seq, tick, idx as u8, total, chunk);
        debug_assert!(scratch.len() <= FRAME_MAX_WIRE_LEN, "frame exceeded the MTU");
        out.push(scratch.clone());
    }
    if out.is_empty() {
        // An empty body is still one stamped frame: `chunks()` yields nothing,
        // but a delta the receiver can count on having seen beats a silent
        // no-op that leaves `frag_total` disagreeing with the wire.
        write_delta_frame(&mut scratch, seq, tick, 0, 1, &[]);
        out.push(scratch.clone());
    }
    debug_assert_eq!(out.len(), total as usize);
    Ok(out.len())
}

/// Forward-only high-water mark over a wrapping `u16` counter.
///
/// Same rule the delta assembler uses: `is_newer` is wrap-safe, so a client
/// that crosses the 65535 boundary does not look like it restarted. Duplicates
/// and late frames are counted, never applied — except that a stale *input* is
/// still reported to the host (flagged in [`Inbound::Input::resupplied`]),
/// because rollback replay needs genuinely late inputs rather than dropping
/// them.
#[derive(Debug, Clone, Copy, Default)]
pub struct SeqWatermark {
    highest: Option<u16>,
    stale: u64,
}

impl SeqWatermark {
    pub fn get(&self) -> Option<u16> {
        self.highest
    }

    /// `true` if `seq` was newer and is now the mark. `false` = stale, counted.
    pub fn advance(&mut self, seq: u16) -> bool {
        match self.highest {
            Some(last) if !is_newer(last, seq) => {
                self.stale += 1;
                false
            }
            _ => {
                self.highest = Some(seq);
                true
            }
        }
    }

    /// `true` if `seq` would not move the mark.
    pub fn is_stale(&self, seq: u16) -> bool {
        self.highest.is_some_and(|last| !is_newer(last, seq))
    }

    pub fn stale(&self) -> u64 {
        self.stale
    }

    pub fn reset(&mut self) {
        *self = SeqWatermark::default();
    }
}

/// One delta's frames, kept together so backpressure can discard a whole
/// superseded delta instead of leaving the receiver holding its tail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundDelta {
    /// `None` for a raw write that is not a stamped delta.
    pub seq: Option<u16>,
    pub tick: Option<u32>,
    pub frames: VecDeque<Vec<u8>>,
}

impl OutboundDelta {
    pub fn frames(&self) -> usize {
        self.frames.len()
    }
}

/// Bounded, drop-oldest send queue for one lane of one session.
///
/// `str0m`'s `write` returns `Ok(false)` when its SCTP buffer is full. That is
/// the signal to start holding frames here; once the queue is full, the
/// *oldest* delta leaves and the newest takes its place. A 128 Hz server has no
/// use for a stale world state, and blocking the mio poll loop for one slow
/// client would stall every other session in the thread.
#[derive(Debug, Default)]
pub struct OutboundQueue {
    items: VecDeque<OutboundDelta>,
    frames: usize,
    dropped_deltas: u64,
    dropped_frames: u64,
}

impl OutboundQueue {
    /// Queue one delta, discarding older ones to stay within `capacity` frames.
    /// Returns how many deltas and how many frames were discarded.
    pub fn push(&mut self, item: OutboundDelta, capacity: usize) -> (u64, u64) {
        // A delta bigger than the entire queue is its own verdict: it cannot be
        // held, so it is counted and refused rather than evicting everything and
        // still not fitting.
        if item.frames() > capacity {
            self.dropped_deltas += 1;
            self.dropped_frames += item.frames() as u64;
            return (1, item.frames() as u64);
        }
        let mut dropped_deltas = 0u64;
        let mut dropped_frames = 0u64;
        while self.frames + item.frames() > capacity {
            let Some(evicted) = self.items.pop_front() else {
                break;
            };
            self.frames -= evicted.frames();
            dropped_deltas += 1;
            dropped_frames += evicted.frames() as u64;
        }
        self.dropped_deltas += dropped_deltas;
        self.dropped_frames += dropped_frames;
        self.frames += item.frames();
        self.items.push_back(item);
        (dropped_deltas, dropped_frames)
    }

    /// Restore a partially written delta to the head after a drain stopped
    /// mid-message, so the lane's order survives the saturation.
    pub fn push_front(&mut self, item: OutboundDelta) {
        self.frames += item.frames();
        self.items.push_front(item);
    }

    /// Discard queued deltas that are no longer newer than `seq`, from the front
    /// of the queue in, and report how many left. Newest wins on the unreliable
    /// lane: a tick that has already been overtaken is worth less than the queue
    /// space it occupies. Raw writes without a `seq` keep their place, since
    /// nothing about them has been superseded.
    pub fn purge_older_than(&mut self, seq: u16) -> usize {
        let mut dropped = 0;
        while let Some(front) = self.items.front() {
            let Some(queued) = front.seq else { break };
            if queued != seq && !is_newer(queued, seq) {
                break;
            }
            let item = self.items.pop_front().expect("front just checked");
            self.frames -= item.frames();
            dropped += 1;
        }
        dropped
    }

    pub fn pop(&mut self) -> Option<OutboundDelta> {
        let item = self.items.pop_front()?;
        self.frames -= item.frames();
        Some(item)
    }

    pub fn peek(&self) -> Option<&OutboundDelta> {
        self.items.front()
    }

    /// Frames the queue is holding right now.
    pub fn frames(&self) -> usize {
        self.frames
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn clear(&mut self) -> usize {
        let count = self.items.iter().map(|d| d.frames()).sum();
        self.items.clear();
        self.frames = 0;
        count
    }

    pub fn dropped_deltas(&self) -> u64 {
        self.dropped_deltas
    }

    pub fn dropped_frames(&self) -> u64 {
        self.dropped_frames
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_webrtc_protocol::{
        DELTA_HEADER_LEN, FragmentAssembler, MAX_DELTA_CHUNK, OP_ACK, OP_INPUT, encode_event,
        parse_ack, write_ack, AckKind, DeltaHeader, EventFrame, EventKind,
    };

    fn frame(seq: u16, tick: u32, idx: u8, total: u8, chunk: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        write_delta_frame(&mut out, seq, tick, idx, total, chunk);
        out
    }

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

    #[test]
    fn ack_round_trips_and_is_length_strict() {
        let mut wire = Vec::new();
        write_ack(&mut wire, 91, AckKind::Event);
        assert_eq!(wire.len(), engine_webrtc_protocol::ACK_WIRE_LEN);
        assert_eq!(
            parse_ack(&wire),
            Some(Ack {
                seq: 91,
                kind: AckKind::Event
            })
        );
        assert_eq!(classify_inbound(Some(Lane::Reliable), &wire), Inbound::Ack(Ack { seq: 91, kind: AckKind::Event }));

        // A longer payload that merely starts with the ack opcode is not an ack.
        let mut padded = wire.clone();
        padded.push(0);
        assert!(parse_ack(&padded).is_none());
        assert!(matches!(
            classify_inbound(Some(Lane::Reliable), &padded),
            Inbound::Refused(RejectReason::Malformed { op: OP_ACK, .. })
        ));
        // An unknown ack kind is not an ack either.
        let mut bogus = wire.clone();
        bogus[3] = 7;
        assert!(parse_ack(&bogus).is_none());
    }

    #[test]
    fn an_ack_on_the_wrong_lane_is_refused() {
        let mut wire = Vec::new();
        write_ack(&mut wire, 5, AckKind::Event);
        assert_eq!(
            classify_inbound(Some(Lane::Unreliable), &wire),
            Inbound::Refused(RejectReason::WrongLane {
                expected: Lane::Reliable,
                got: Lane::Unreliable
            })
        );
    }

    fn input(seq: u16) -> Vec<u8> {
        let mut buf = vec![OP_INPUT];
        buf.extend_from_slice(&seq.to_le_bytes());
        buf.resize(INPUT_WIRE_LEN, 0x11);
        buf
    }

    #[test]
    fn inputs_are_parsed_on_the_unreliable_lane_only() {
        let buf = input(4);
        match classify_inbound(Some(Lane::Unreliable), &buf) {
            Inbound::Input {
                seq,
                payload,
                resupplied,
            } => {
                assert_eq!(seq, 4);
                assert!(!resupplied);
                assert_eq!(payload.as_slice(), &[0x11u8; INPUT_PAYLOAD_LEN]);
            }
            other => panic!("expected an input, got {other:?}"),
        }
        assert!(matches!(
            classify_inbound(Some(Lane::Reliable), &buf),
            Inbound::Refused(RejectReason::WrongLane {
                expected: Lane::Unreliable,
                got: Lane::Reliable
            })
        ));
        assert!(matches!(
            classify_inbound(None, &buf),
            Inbound::Refused(RejectReason::OffContract)
        ));
        // 22 bytes is not an input frame.
        assert!(matches!(
            classify_inbound(Some(Lane::Unreliable), &buf[..22]),
            Inbound::Refused(RejectReason::Malformed {
                op: OP_INPUT,
                got: 22,
                want: INPUT_WIRE_LEN
            })
        ));
    }

    #[test]
    fn a_client_delta_frame_is_never_accepted() {
        let buf = frame(3, 4, 0, 1, &[9]);
        assert!(matches!(
            classify_inbound(Some(Lane::Unreliable), &buf),
            Inbound::Refused(RejectReason::Malformed { op: OP_DELTA, .. })
        ));
    }

    #[test]
    fn events_are_accepted_on_the_reliable_lane_only() {
        let mut raw = Vec::new();
        let event = EventFrame {
            seq: 1,
            kind: EventKind::Fire,
            from: 3,
            target: 4,
            tick: 12,
            value: 1.0,
        };
        encode_event(&event, &mut raw);
        assert!(matches!(
            classify_inbound(Some(Lane::Reliable), &raw),
            Inbound::Event { .. }
        ));
        assert!(matches!(
            classify_inbound(Some(Lane::Unreliable), &raw),
            Inbound::Refused(RejectReason::WrongLane { .. })
        ));
    }

    #[test]
    fn exact_labels_are_the_contract_and_other_labels_are_ignored() {
        assert_eq!(classify_label(LABEL_UNRELIABLE), LabelVerdict::Contract(Lane::Unreliable));
        assert_eq!(classify_label(LABEL_RELIABLE), LabelVerdict::Contract(Lane::Reliable));

        // Never a silent fallback to the reliable lane.
        assert_eq!(classify_label("telemetry"), LabelVerdict::Ignored);
        assert_eq!(classify_label("game-chat"), LabelVerdict::Ignored);
        assert_eq!(classify_label(""), LabelVerdict::Ignored);
        assert_eq!(classify_label("GAME-RELIABLE"), LabelVerdict::Ignored);
        assert_eq!(classify_label("game-reliable-v2"), LabelVerdict::Ignored);

        // Documented legacy aliases, including the bare lane words older bundles used.
        assert_eq!(classify_label("unreliable"), LabelVerdict::LegacyAlias(Lane::Unreliable));
        assert_eq!(classify_label("reliable"), LabelVerdict::LegacyAlias(Lane::Reliable));
        assert_eq!(classify_label("client-unreliable"), LabelVerdict::LegacyAlias(Lane::Unreliable));
        assert_eq!(classify_label("client_reliable"), LabelVerdict::LegacyAlias(Lane::Reliable));
        // "unreliable" ends with "reliable": the ordering trap, caught.
        assert_ne!(classify_label("client-unreliable"), LabelVerdict::LegacyAlias(Lane::Reliable));
    }

    #[test]
    fn stale_seq_is_counted_and_never_moves_the_mark_backwards() {
        let mut mark = SeqWatermark::default();
        assert_eq!(mark.get(), None);
        assert!(mark.advance(0), "the first seq is always fresh");
        assert_eq!(mark.get(), Some(0));

        assert!(mark.advance(1));
        assert!(mark.advance(2));
        assert_eq!(mark.stale(), 0);

        // Duplicate and late are both stale.
        assert!(!mark.advance(2));
        assert!(!mark.advance(1));
        assert!(mark.is_stale(1));
        assert_eq!(mark.stale(), 2);
        assert_eq!(mark.get(), Some(2), "the mark only moves forward");

        // Wrap-safe, exactly like the host's newest-wins rule: from 2, the number
        // space runs the other way, so 65535 is *behind* 2 and 0 is ahead of 65535.
        assert!(!mark.advance(u16::MAX), "65535 is behind 2 in the wrap window");
        assert!(mark.advance(3));
    }

    #[test]
    fn the_watermark_wraps_forward_once_and_rejects_the_old_side() {
        let mut mark = SeqWatermark::default();
        assert!(mark.advance(65534));
        assert!(mark.advance(65535));
        assert!(!mark.advance(65535), "duplicate");
        assert!(mark.advance(0), "the counter wraps forward");
        assert_eq!(mark.get(), Some(0));
        assert!(!mark.advance(65535), "the pre-wrap seq is behind again");
        assert!(!mark.advance(0), "duplicate after the wrap");
        assert_eq!(mark.stale(), 3);
    }

    #[test]
    fn backpressure_drops_the_oldest_delta_whole() {
        let mut queue = OutboundQueue::default();
        let one = |seq: u16| OutboundDelta {
            seq: Some(seq),
            tick: Some(seq as u32),
            frames: VecDeque::from(vec![frame(seq, 1, 0, 1, &[seq as u8; 100]); 1]),
        };

        // Fill to capacity with single-frame deltas.
        for s in 0..MAX_QUEUED_FRAMES as u16 {
            assert_eq!(queue.push(one(s), MAX_QUEUED_FRAMES), (0, 0));
        }
        assert_eq!(queue.frames(), MAX_QUEUED_FRAMES);
        assert_eq!(queue.dropped_deltas(), 0);

        // One more evicts exactly the oldest.
        assert_eq!(queue.push(one(2000), MAX_QUEUED_FRAMES), (1, 1));
        assert_eq!(queue.dropped_deltas(), 1);
        assert_eq!(queue.dropped_frames(), 1);
        assert_eq!(queue.frames(), MAX_QUEUED_FRAMES);
        assert_eq!(queue.peek().unwrap().seq, Some(1), "newest wins");
        assert_eq!(
            queue.items.iter().map(|d| d.seq.unwrap()).max().unwrap(),
            2000
        );
    }

    /// A whole delta is evicted rather than half of it, so the receiver is not
    /// left holding the tail of a message whose head already left.
    #[test]
    fn a_multi_frame_delta_is_evicted_as_one_unit() {
        let mut queue = OutboundQueue::default();
        let fat = OutboundDelta {
            seq: Some(1),
            tick: Some(1),
            frames: VecDeque::from(vec![frame(1, 1, 0, 2, &[0u8; 10]); 2]),
        };
        assert_eq!(queue.push(fat, 3), (0, 0));
        assert_eq!(queue.frames(), 2);

        let newer = OutboundDelta {
            seq: Some(2),
            tick: Some(2),
            frames: VecDeque::from(vec![frame(2, 2, 0, 1, &[0u8; 10]); 2]),
        };
        assert_eq!(queue.push(newer, 3), (1, 2), "the older delta left entirely");
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.frames(), 2);
        assert_eq!(queue.peek().unwrap().seq, Some(2));
        assert_eq!(queue.dropped_frames(), 2);
    }

    #[test]
    fn a_delta_bigger_than_the_queue_is_refused_rather_than_evicting_everything() {
        let mut queue = OutboundQueue::default();
        queue.push(
            OutboundDelta {
                seq: Some(1),
                tick: Some(1),
                frames: VecDeque::from(vec![frame(1, 1, 0, 1, &[0u8; 4]); 1]),
            },
            8,
        );
        let too_big = OutboundDelta {
            seq: Some(2),
            tick: Some(2),
            frames: VecDeque::from(vec![frame(2, 2, 0, 9, &[0u8; 4]); 9]),
        };
        assert_eq!(queue.push(too_big, 8), (1, 9));
        assert_eq!(queue.len(), 1, "the queued delta survived");
        assert_eq!(queue.peek().unwrap().seq, Some(1));
        assert_eq!(queue.dropped_deltas(), 1);
    }

    /// Supersession discards the deltas a newer tick replaced, from the front in,
    /// and stops at the first one that is still newer.
    #[test]
    fn superseded_deltas_leave_the_queue_oldest_first() {
        let mut queue = OutboundQueue::default();
        for s in [10u16, 11, 12] {
            queue.push(
                OutboundDelta {
                    seq: Some(s),
                    tick: Some(s as u32),
                    frames: VecDeque::from(vec![frame(s, 1, 0, 1, &[0u8; 4])]),
                },
                8,
            );
        }
        assert_eq!(queue.purge_older_than(11), 2, "10 and 11 itself were both replaced");
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.peek().unwrap().seq, Some(12));
        assert_eq!(queue.frames(), 1);

        // A raw write with no seq has not been superseded by anything.
        queue.push_front(OutboundDelta {
            seq: None,
            tick: None,
            frames: VecDeque::from(vec![frame(12, 1, 0, 1, &[0u8; 4])]),
        });
        assert_eq!(queue.purge_older_than(99), 0);
        assert_eq!(queue.frames(), 2);
    }

    /// Wrap: 0 is newer than 65535, so a delta at 0 supersedes it.
    #[test]
    fn supersession_is_wrap_safe() {
        let mut queue = OutboundQueue::default();
        queue.push(
            OutboundDelta {
                seq: Some(u16::MAX),
                tick: Some(1),
                frames: VecDeque::from(vec![frame(u16::MAX, 1, 0, 1, &[0u8; 4])]),
            },
            8,
        );
        assert_eq!(queue.purge_older_than(0), 1);
        assert!(queue.is_empty());
    }

    #[test]
    fn popping_drains_in_fifo_order_and_clearing_counts_frames() {
        let mut queue = OutboundQueue::default();
        for s in 1..=3u16 {
            queue.push(
                OutboundDelta {
                    seq: Some(s),
                    tick: Some(s as u32),
                    frames: VecDeque::from(vec![frame(s, 1, 0, 1, &[s as u8; 8])]),
                },
                8,
            );
        }
        assert_eq!(queue.pop().unwrap().seq, Some(1));
        assert_eq!(queue.frames(), 2);
        assert_eq!(queue.clear(), 2);
        assert!(queue.is_empty());
        assert_eq!(queue.frames(), 0);
        assert_eq!(queue.pop(), None);
    }

    #[test]
    fn sctp_parameters_must_match_the_lane() {
        let unreliable_cfg = ChannelConfig {
            label: LABEL_UNRELIABLE.to_string(),
            ordered: false,
            reliability: Reliability::MaxRetransmits { retransmits: 0 },
            negotiated: None,
            protocol: String::new(),
        };
        let reliable_cfg = ChannelConfig {
            label: LABEL_RELIABLE.to_string(),
            ordered: true,
            reliability: Reliability::Reliable,
            negotiated: None,
            protocol: String::new(),
        };
        assert_eq!(contract_mismatch(Lane::Unreliable, &unreliable_cfg), None);
        assert_eq!(contract_mismatch(Lane::Reliable, &reliable_cfg), None);

        // A browser bundle that opened 'game-unreliable' with defaults is not
        // an unreliable lane, and the transport must say so.
        assert!(contract_mismatch(Lane::Unreliable, &reliable_cfg).is_some());
        assert!(contract_mismatch(Lane::Reliable, &unreliable_cfg).is_some());
    }

    #[test]
    fn input_body_extraction_matches_the_host_payload_layout() {
        let buf = input(300);
        assert_eq!(
            input_body(&buf).as_ref().map(|b| &b[..]),
            Some(&buf[HEADER_LEN..])
        );
        assert_eq!(input_body(&buf[..10]), None);
    }
}
