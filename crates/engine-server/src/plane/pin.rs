//! Thread→core affinity via raw `sched_setaffinity`.
//!
//! `libc` exposes `cpu_set_t` but not the `CPU_ZERO`/`CPU_SET` macros (they
//! are header macros), so the bitmask is built by hand from `__bits`.
//! One core per sim thread is the point: the 128 Hz budget assumes a private
//! L1 and no migration.

use std::io;

/// Affinity is a deployment nicety, not a correctness requirement: on
/// non-Linux (dev machines, CI, Windows/macOS servers) pinning is a no-op and
/// the scheduler does its usual thing.
#[cfg(not(target_os = "linux"))]
pub fn pin_current_thread(_core: usize) -> io::Result<()> {
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn current_core() -> i32 {
    // Same convention as the Linux probe: -1 = unknown.
    -1
}

#[cfg(target_os = "linux")]
pub fn pin_current_thread(core: usize) -> io::Result<()> {
    // glibc's cpu_set_t is a 1024-bit mask in 16 u64 words; the libc crate's
    // field is hygienically named, so reach the storage as a plain word array.
    const WORDS: usize = std::mem::size_of::<libc::cpu_set_t>() / std::mem::size_of::<u64>();
    let mut mask = [0u64; WORDS];
    if core >= WORDS * 64 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "core id out of range"));
    }
    mask[core / 64] |= 1u64 << (core % 64);
    let rc = unsafe {
        libc::sched_setaffinity(
            0,
            std::mem::size_of::<libc::cpu_set_t>(),
            mask.as_ptr() as *const libc::cpu_set_t,
        )
    };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub fn current_core() -> i32 {
    let rc = unsafe { libc::sched_getcpu() };
    if rc < 0 { -1 } else { rc }
}
