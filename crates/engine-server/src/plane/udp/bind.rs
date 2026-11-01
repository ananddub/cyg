use std::io;
use std::net::SocketAddr;

#[cfg(unix)]
pub(super) fn bind_udp(addr: SocketAddr) -> io::Result<std::net::UdpSocket> {
    use std::os::unix::io::FromRawFd;
    let domain = if addr.is_ipv6() {
        libc::AF_INET6
    } else {
        libc::AF_INET
    };
    let fd = unsafe {
        libc::socket(
            domain,
            libc::SOCK_DGRAM | libc::SOCK_NONBLOCK,
            libc::IPPROTO_UDP,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let sock = unsafe { std::net::UdpSocket::from_raw_fd(fd) };
    let on: libc::c_int = 1;
    let len = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
    unsafe {
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_REUSEADDR,
            (&on as *const libc::c_int).cast(),
            len,
        );
        #[cfg(any(
            target_os = "linux",
            target_os = "android",
            target_os = "macos",
            target_os = "ios",
            target_os = "freebsd",
            target_os = "dragonfly",
            target_os = "netbsd",
            target_os = "openbsd"
        ))]
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_REUSEPORT,
            (&on as *const libc::c_int).cast(),
            len,
        );
    }
    let rc = match addr {
        SocketAddr::V4(a) => {
            let mut sa: libc::sockaddr_in = unsafe { std::mem::zeroed() };
            sa.sin_family = libc::AF_INET as libc::sa_family_t;
            sa.sin_port = a.port().to_be();
            sa.sin_addr = libc::in_addr {
                s_addr: u32::from(a.ip().to_bits()).to_be(),
            };
            unsafe {
                libc::bind(
                    fd,
                    (&sa as *const libc::sockaddr_in).cast(),
                    std::mem::size_of_val(&sa) as libc::socklen_t,
                )
            }
        }
        SocketAddr::V6(a) => {
            let mut sa: libc::sockaddr_in6 = unsafe { std::mem::zeroed() };
            sa.sin6_family = libc::AF_INET6 as libc::sa_family_t;
            sa.sin6_port = a.port().to_be();
            sa.sin6_addr = libc::in6_addr {
                s6_addr: a.ip().octets(),
            };
            sa.sin6_scope_id = a.scope_id();
            unsafe {
                libc::bind(
                    fd,
                    (&sa as *const libc::sockaddr_in6).cast(),
                    std::mem::size_of_val(&sa) as libc::socklen_t,
                )
            }
        }
    };
    if rc < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(sock)
}

#[cfg(not(unix))]
pub(super) fn bind_udp(addr: SocketAddr) -> io::Result<std::net::UdpSocket> {
    let sock = std::net::UdpSocket::bind(addr)?;
    sock.set_nonblocking(true)?;
    Ok(sock)
}
