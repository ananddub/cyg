
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

static ALLOCS: AtomicU64 = AtomicU64::new(0);
static DEALLOCS: AtomicU64 = AtomicU64::new(0);

struct CountingAlloc;

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        DEALLOCS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

static MEASURE_LOCK: Mutex<()> = Mutex::new(());

fn snapshot() -> (u64, u64) {
    (ALLOCS.load(Ordering::Relaxed), DEALLOCS.load(Ordering::Relaxed))
}

fn min_allocs_per_iter<F: FnMut()>(windows: usize, iterations: usize, mut f: F) -> f64 {
    let mut min_a = u64::MAX;
    for _ in 0..windows {
        let (a0, _) = snapshot();
        for _ in 0..iterations {
            f();
        }
        let (a1, _) = snapshot();
        min_a = min_a.min(a1 - a0);
    }
    min_a as f64 / iterations as f64
}

use engine_webrtc_protocol::channel::{DeltaPacker, FragmentAssembler, MAX_DELTA_CHUNK};

#[test]
fn flat_delta_cycle_is_a_fixed_tiny_alloc_count() {
    let _guard = MEASURE_LOCK.lock().unwrap();

    let body_len = 2 * MAX_DELTA_CHUNK + 44;
    let body: Vec<u8> = (0..body_len).map(|i| (i % 251) as u8).collect();
    let mut wire = Vec::with_capacity(3 * (DELTA_HEADER + MAX_DELTA_CHUNK));
    let mut out = Vec::new();

    let mut cycle = || {
        let mut packer = DeltaPacker::new(777, 1);
        let packing = packer.pack(&body).expect("packs");
        wire.clear();
        packing.append_frames_to(&mut wire);

        let mut assembler = FragmentAssembler::default();
        let mut pos = 0usize;
        let mut delivered = None;
        for fragment in packing.fragments() {
            let flen = DELTA_HEADER + fragment.chunk.len();
            let (header, chunk) = engine_webrtc_protocol::channel::DeltaHeader::parse(
                &wire[pos..pos + flen],
            )
            .expect("parses");
            if let Some(delta) = assembler.insert(&header, chunk, 0) {
                delivered = Some(delta.body);
            }
            pos += flen;
        }
        let body = delivered.expect("complete delta delivers");
        out.clear();
        out.extend_from_slice(&body);
        black_box(&out);
    };

    for _ in 0..128 {
        cycle();
    }
    let per_iter = min_allocs_per_iter(8, 256, cycle);
    assert!(
        per_iter <= 3.0,
        "flat delta cycle allocates {per_iter}/iter (budget 3) — the zero-copy assembler regressed"
    );
}

use std::hint::black_box;

const DELTA_HEADER: usize = 9;

#[test]
fn delta_cycle_window_is_leak_free() {
    let _guard = MEASURE_LOCK.lock().unwrap();
    let body_len = 2 * MAX_DELTA_CHUNK + 44;
    let body: Vec<u8> = (0..body_len).map(|i| (i % 241) as u8).collect();
    let mut wire = Vec::with_capacity(3 * (DELTA_HEADER + MAX_DELTA_CHUNK));
    let mut seq = 10_000u16;

    {
        let mut packer = DeltaPacker::new(seq.wrapping_sub(1), 1);
        let packing = packer.pack(&body).expect("packs");
        wire.clear();
        packing.append_frames_to(&mut wire);
        let mut assembler = FragmentAssembler::default();
        feed_all(&mut assembler, &packing, &wire);
    }

    let (a0, d0) = snapshot();
    for _ in 0..2_000 {
        seq = seq.wrapping_add(1);
        let mut packer = DeltaPacker::new(seq, 1);
        let packing = packer.pack(&body).expect("packs");
        wire.clear();
        packing.append_frames_to(&mut wire);

        let mut assembler = FragmentAssembler::default();
        feed_all(&mut assembler, &packing, &wire);
    }
    let (a1, d1) = snapshot();
    let net = (a1 - a0) as i64 - (d1 - d0) as i64;
    assert!(
        net.abs() <= 64,
        "net {net} over 2,000 delta cycles — leak in the channel layer"
    );
}

use engine_webrtc_protocol::channel::{DeltaHeader, DeltaPacking};

fn feed_all(assembler: &mut FragmentAssembler, packing: &DeltaPacking, wire: &[u8]) {
    let mut pos = 0usize;
    for fragment in packing.fragments() {
        let flen = DELTA_HEADER + fragment.chunk.len();
        let (header, chunk) =
            DeltaHeader::parse(&wire[pos..pos + flen]).expect("fragment frame parses");
        let _ = assembler.insert(&header, chunk, 0);
        pos += flen;
    }
}
