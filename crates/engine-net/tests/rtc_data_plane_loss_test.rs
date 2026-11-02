
use std::collections::{HashMap, HashSet};

use engine_webrtc_protocol::channel::{
    DeltaHeader, FRAGMENT_TTL_MS, FragmentAssembler, MAX_DELTA_BODY, MAX_DELTA_CHUNK,
    delta_fragment_count, write_delta_frame,
};
use engine_webrtc_protocol::frames::is_newer;

const SEED: u32 = 0xC0FF_EE;
const TICKS: u32 = 400;
const TICK_MS: u64 = 8;
const LOSS_PCT: u64 = 10;
const JITTER_MS: u64 = 151;

struct Rng(u32);

impl Rng {
    fn next(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
}

fn body_len(t: u32) -> usize {
    match t % 4 {
        0 => 300,
        1 => 1500,
        2 => 2500,
        _ => 50,
    }
}

fn delta_body(t: u32) -> Vec<u8> {
    let len = body_len(t);
    let mut body = vec![0u8; len];
    body[0] = 0xa5;
    body[1..6].copy_from_slice(b"delta");
    body[6..14].copy_from_slice(&7i64.to_le_bytes());
    let fill = len - 20;
    for i in 0..fill {
        body[14 + i] = ((i as u32 * 31 + t * 7) % 251) as u8;
    }
    let acked = (t - 1) as u16;
    body[len - 6..len - 4].copy_from_slice(&acked.to_le_bytes());
    body[len - 4..].copy_from_slice(&t.to_le_bytes());
    body
}

struct FragEvent {
    arrival_ms: u64,
    seq: u16,
    wire: Vec<u8>,
}

struct Pipe {
    events: Vec<FragEvent>,
    expected: HashMap<u16, Vec<u8>>,

    mangled: HashSet<u16>,
    fragments_total: usize,
    fragments_dropped: usize,
}

fn build_pipe() -> Pipe {
    let mut rng = Rng(SEED);
    let mut events = Vec::new();
    let mut expected = HashMap::new();
    let mut mangled = HashSet::new();
    let (mut fragments_total, mut fragments_dropped) = (0usize, 0usize);

    for t in 1..=TICKS {
        let body = delta_body(t);
        let seq = t as u16;
        expected.insert(seq, body.clone());
        let total = delta_fragment_count(body.len()).expect("body must be carryable");
        let emit_ms = t as u64 * TICK_MS;
        for (idx, chunk) in body.chunks(MAX_DELTA_CHUNK).enumerate() {

            let drop_roll = rng.next() as u64 % 100;
            let jitter = rng.next() as u64 % JITTER_MS;
            fragments_total += 1;
            if drop_roll < LOSS_PCT {
                fragments_dropped += 1;
                mangled.insert(seq);
                continue;
            }
            let mut wire = Vec::new();
            write_delta_frame(&mut wire, seq, t, idx as u8, total, chunk);
            events.push(FragEvent {
                arrival_ms: emit_ms + jitter,
                seq,
                wire,
            });
        }
    }

    events.sort_by_key(|e| (e.arrival_ms, e.seq, e.wire[7]));
    Pipe {
        events,
        expected,
        mangled,
        fragments_total,
        fragments_dropped,
    }
}

#[derive(Default)]
struct Delivered {
    seqs: Vec<u16>,
    stale_after: u64,
}

fn run_pipe(pipe: &Pipe) -> Delivered {
    let mut assembler = FragmentAssembler::default();
    let mut out = Delivered::default();
    for event in &pipe.events {

        assembler.cleanup(event.arrival_ms);
        let (header, chunk) = DeltaHeader::parse(&event.wire).expect("pipe frames parse");
        if let Some(delta) = assembler.insert(&header, chunk, event.arrival_ms) {
            out.seqs.push(delta.seq);
        }
    }
    out.stale_after = assembler.stale();
    out
}

#[test]
fn lossy_pipe_delivers_complete_sequenced_deltas() {
    let pipe = build_pipe();
    let got = run_pipe(&pipe);

    let mut byte_exact = true;
    let mut monotonic = true;
    let mut no_duplicates = true;
    let mut mangled_delivered = 0;
    let mut seen: HashSet<u16> = HashSet::new();
    let mut last: Option<u16> = None;

    for &seq in &got.seqs {
        if !seen.insert(seq) {
            no_duplicates = false;
        }
        if pipe.mangled.contains(&seq) {
            mangled_delivered += 1;
        }

        if let Some(prev) = last {
            if !is_newer(prev, seq) {
                monotonic = false;
            }
        }
        last = Some(seq);
    }

    let mut assembler = FragmentAssembler::default();
    let mut bodies: Vec<(u16, Vec<u8>)> = Vec::new();
    for event in &pipe.events {
        assembler.cleanup(event.arrival_ms);
        let (header, chunk) = DeltaHeader::parse(&event.wire).unwrap();
        if let Some(delta) = assembler.insert(&header, chunk, event.arrival_ms) {
            bodies.push((delta.seq, delta.body));
        }
    }
    for (seq, body) in &bodies {
        if body != &pipe.expected[seq] {
            byte_exact = false;
        }

        let t = *seq as u32;
        if body.len() >= 4 && body[body.len() - 4..] != t.to_le_bytes() {
            byte_exact = false;
        }
    }
    assert_eq!(bodies.len(), got.seqs.len());

    let whole: HashSet<u16> = pipe
        .expected
        .keys()
        .filter(|s| !pipe.mangled.contains(s))
        .copied()
        .collect();
    let delivered: HashSet<u16> = got.seqs.iter().copied().collect();
    let starved: Vec<u16> = whole
        .iter()
        .filter(|s| !delivered.contains(s))
        .copied()
        .collect();

    println!(
        "--- rtc loss pipe (Rust, seed 0x{SEED:X}, {LOSS_PCT}% drop, ≤{JITTER_MS} ms jitter) ---"
    );
    println!(
        "ticks={TICKS} fragments={} dropped={} delivered={} stale_frames={} intact_but_superseded={} last_delivered={}",
        pipe.fragments_total,
        pipe.fragments_dropped,
        got.seqs.len(),
        got.stale_after,
        starved.len(),
        got.seqs.last().copied().unwrap_or(0),
    );
    if !starved.is_empty() {
        let sample: Vec<u16> = starved.iter().take(8).copied().collect();
        println!("starved seqs (sample): {sample:?}");
    }
    println!(
        "PASS byte-exact={byte_exact} monotonic={monotonic} no-duplicates={no_duplicates} mangled-never-delivered={}",
        mangled_delivered == 0
    );

    assert!(
        byte_exact,
        "a delivered delta must be the server's exact bytes"
    );
    assert!(
        monotonic,
        "delivered seqs must strictly increase (wrap-safe)"
    );
    assert!(
        no_duplicates,
        "a delta must reach the game layer at most once"
    );
    assert_eq!(
        mangled_delivered, 0,
        "a delta missing a fragment must never be delivered"
    );
    assert!(
        got.stale_after > 0,
        "with reordering, stale late frames must have been exercised"
    );

    assert!(!got.seqs.is_empty(), "the pipe must deliver something");
    assert!(
        *got.seqs.last().unwrap() >= (TICKS - 16) as u16,
        "the newest ticks must still reach a client under this jitter, last delivered {}",
        got.seqs.last().unwrap()
    );
}

#[test]
fn ttl_evicts_a_stalled_partial() {
    let mut assembler = FragmentAssembler::default();
    let mut wire = Vec::new();
    write_delta_frame(&mut wire, 10, 10, 0, 3, &[0xa5]);
    let (header, chunk) = DeltaHeader::parse(&wire).unwrap();
    assert!(assembler.insert(&header, chunk, 1_000).is_none());

    assert!(
        !assembler.cleanup(1_000 + FRAGMENT_TTL_MS),
        "TTL is strict 'greater than'"
    );
    assert!(
        assembler.cleanup(1_000 + FRAGMENT_TTL_MS + 1),
        "stalled chunks must be evicted"
    );

    let mut wire1 = Vec::new();
    write_delta_frame(&mut wire1, 10, 10, 1, 3, &[0xb5]);
    let (h1, c1) = DeltaHeader::parse(&wire1).unwrap();
    assert!(assembler.insert(&h1, c1, 5_000).is_none());
    let mut wire2 = Vec::new();
    write_delta_frame(&mut wire2, 10, 10, 2, 3, &[0xc5]);
    let (h2, c2) = DeltaHeader::parse(&wire2).unwrap();
    assert!(
        assembler.insert(&h2, c2, 5_001).is_none(),
        "fragment 0 was evicted, so this must not close"
    );
    assert_eq!(assembler.assembled(), 0);
}

#[test]
fn backpressure_newest_wins_for_a_stalled_consumer() {

    let mut assembler = FragmentAssembler::default();
    let mut applied: Option<(u16, u32)> = None;

    let mut feed = |seq: u16, now: u64, assembler: &mut FragmentAssembler| {
        let mut wire = Vec::new();
        write_delta_frame(&mut wire, seq, seq as u32, 0, 1, &[0xa5, seq as u8]);
        let (header, chunk) = DeltaHeader::parse(&wire).unwrap();
        if let Some(delta) = assembler.insert(&header, chunk, now) {
            applied = Some((delta.seq, delta.tick));
        }
    };

    for seq in 100..120 {
        feed(seq, seq as u64 * TICK_MS, &mut assembler);
    }
    feed(101, 2_000, &mut assembler);
    feed(105, 2_010, &mut assembler);
    feed(119, 2_020, &mut assembler);
    feed(120, 2_030, &mut assembler);

    assert_eq!(
        applied,
        Some((120, 120)),
        "the consumer must land on the newest tick"
    );
    assert_eq!(
        assembler.assembled(),
        21,
        "100..120 delivered once each, the 119 retransmit not twice"
    );
    assert!(
        assembler.stale() >= 3,
        "late stragglers must be counted stale, got {}",
        assembler.stale()
    );
    println!(
        "backpressure (codec surface): newest tick applied, {} stale frames dropped, queue length 1",
        assembler.stale()
    );
}

#[test]
fn oversized_delta_is_dropped_not_truncated() {
    assert!(delta_fragment_count(MAX_DELTA_BODY + 1).is_none());
    assert!(delta_fragment_count(MAX_DELTA_BODY).is_some());
}

#[test]
fn server_egress_backpressure_is_blocked() {

    println!(
        "SKIP server-side egress backpressure: send_unreliable/broadcast_unreliable \
         report only success or a datagram count, so there is no per-channel queue \
         depth to assert a bounded drop against. Once the transport exposes a buffered \
         amount, extend the pipe test to feed delta bodies through \
         engine_server::plane::emit_delta and assert that a per-peer bounded queue \
         drops the oldest frames instead of growing."
    );
}
