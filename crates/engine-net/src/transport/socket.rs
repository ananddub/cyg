use std::net::SocketAddr;

pub(crate) fn sockaddr_storage_of(addr: &SocketAddr) -> (libc::sockaddr_storage, libc::socklen_t) {
    match addr {
        SocketAddr::V4(v4) => {
            let mut sin: libc::sockaddr_in = unsafe { std::mem::zeroed() };
            sin.sin_family = libc::AF_INET as libc::sa_family_t;
            sin.sin_port = v4.port().to_be();
            sin.sin_addr = libc::in_addr {
                s_addr: u32::from_ne_bytes(v4.ip().octets()),
            };
            let mut storage: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
            storage.ss_family = libc::AF_INET as libc::sa_family_t;
            unsafe {
                std::ptr::copy_nonoverlapping(
                    &sin as *const libc::sockaddr_in as *const u8,
                    &mut storage as *mut libc::sockaddr_storage as *mut u8,
                    std::mem::size_of::<libc::sockaddr_in>(),
                );
            }
            (
                storage,
                std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t,
            )
        }
        SocketAddr::V6(v6) => {
            let mut sin6: libc::sockaddr_in6 = unsafe { std::mem::zeroed() };
            sin6.sin6_family = libc::AF_INET6 as libc::sa_family_t;
            sin6.sin6_port = v6.port().to_be();
            sin6.sin6_flowinfo = v6.flowinfo();
            sin6.sin6_addr = libc::in6_addr {
                s6_addr: v6.ip().octets(),
            };
            sin6.sin6_scope_id = v6.scope_id();
            let mut storage: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
            storage.ss_family = libc::AF_INET6 as libc::sa_family_t;
            unsafe {
                std::ptr::copy_nonoverlapping(
                    &sin6 as *const libc::sockaddr_in6 as *const u8,
                    &mut storage as *mut libc::sockaddr_storage as *mut u8,
                    std::mem::size_of::<libc::sockaddr_in6>(),
                );
            }
            (
                storage,
                std::mem::size_of::<libc::sockaddr_in6>() as libc::socklen_t,
            )
        }
    }
}
