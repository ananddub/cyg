use std::io;

#[cfg(not(target_os = "linux"))]
pub fn pin_current_thread(_core: usize) -> io::Result<()> {
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn current_core() -> i32 {
    -1
}

#[cfg(target_os = "linux")]
pub fn pin_current_thread(core: usize) -> io::Result<()> {
    const WORDS: usize = std::mem::size_of::<libc::cpu_set_t>() / std::mem::size_of::<u64>();
    let mut mask = [0u64; WORDS];
    if core >= WORDS * 64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "core id out of range",
        ));
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

#[cfg(target_os = "linux")]
pub fn current_core() -> i32 {
    let rc = unsafe { libc::sched_getcpu() };
    if rc < 0 { -1 } else { rc }
}
