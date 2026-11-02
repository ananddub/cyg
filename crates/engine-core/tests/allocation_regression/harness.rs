use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

static ALLOCS: AtomicU64 = AtomicU64::new(0);
static DEALLOCS: AtomicU64 = AtomicU64::new(0);

struct CountingAlloc;

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        DEALLOCS.fetch_add(1, Ordering::Relaxed);
        System.dealloc(ptr, layout)
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

pub static MEASURE_LOCK: Mutex<()> = Mutex::new(());

pub fn snapshot() -> (u64, u64) {
    (ALLOCS.load(Ordering::Relaxed), DEALLOCS.load(Ordering::Relaxed))
}

pub fn allocs_in<F: FnMut()>(iterations: usize, mut f: F) -> (u64, u64) {
    let (a0, d0) = snapshot();
    for _ in 0..iterations {
        f();
    }
    let (a1, d1) = snapshot();
    (a1 - a0, d1 - d0)
}

pub fn min_allocs_per_iter<F: FnMut()>(windows: usize, iterations: usize, mut f: F) -> (f64, f64) {
    let mut min_a = u64::MAX;
    let mut min_d = u64::MAX;
    for _ in 0..windows {
        let (a, d) = allocs_in(iterations, &mut f);
        min_a = min_a.min(a);
        min_d = min_d.min(d);
    }
    (
        min_a as f64 / iterations as f64,
        min_d as f64 / iterations as f64,
    )
}

pub const WARMUP: usize = 256;
pub const ITER: usize = 512;
pub const WINDOWS: usize = 8;
