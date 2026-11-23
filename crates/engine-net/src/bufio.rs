use std::io::{self, Read};
use std::net::SocketAddr;

#[cfg(target_os = "linux")]
use std::os::unix::io::AsRawFd;

use bytes::BytesMut;
use mio::net::{TcpStream, UdpSocket};

pub(crate) fn read_tcp(
    stream: &mut TcpStream,
    dst: &mut BytesMut,
    want: usize,
) -> io::Result<usize> {
    dst.reserve(want);
    let spare = dst.spare_capacity_mut();
    let len = spare.len().min(want);

    let buf = unsafe { std::slice::from_raw_parts_mut(spare.as_mut_ptr() as *mut u8, len) };
    match stream.read(buf) {
        Ok(0) => Ok(0),
        Ok(n) => {
            unsafe { dst.set_len(dst.len() + n) };
            Ok(n)
        }
        Err(e) => Err(e),
    }
}

pub(crate) fn recv_udp(
    socket: &mut UdpSocket,
    dst: &mut BytesMut,
    want: usize,
) -> io::Result<Option<(usize, SocketAddr)>> {
    dst.reserve(want);
    let spare = dst.spare_capacity_mut();
    let len = spare.len().min(want);

    let buf = unsafe { std::slice::from_raw_parts_mut(spare.as_mut_ptr() as *mut u8, len) };
    match socket.recv_from(buf) {
        Ok((n, addr)) => {
            unsafe { dst.set_len(dst.len() + n) };
            Ok(Some((n, addr)))
        }
        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
        Err(e) => Err(e),
    }
}

/// Max datagrams pulled per `recvmmsg` call.
pub(crate) const RECV_BATCH_MAX: usize = 64;
/// Per-datagram scratch buffer size (covers a 1500-byte MTU packet comfortably).
const RECV_BATCH_BUF_SIZE: usize = 2048;

/// Reusable scratch space for `recv_udp_batch`, owned by the transport so that
/// steady-state receives allocate nothing.
pub(crate) struct RecvBatchScratch {
    /// One buffer per batch slot; received bytes land in `bufs[i][..lens[i]]`.
    pub(crate) bufs: Vec<Vec<u8>>,
    pub(crate) lens: Vec<usize>,
    pub(crate) addrs: Vec<SocketAddr>,
    /// Datagrams dropped because they exceeded the per-slot buffer (MSG_TRUNC).
    pub(crate) truncated: u64,
    #[cfg(target_os = "linux")]
    iovs: Vec<libc::iovec>,
    #[cfg(target_os = "linux")]
    addrs_raw: Vec<libc::sockaddr_storage>,
    #[cfg(target_os = "linux")]
    msgs: Vec<libc::mmsghdr>,
}

// SAFETY: the raw pointers inside the libc iovec/msghdr entries all point into
// this struct's own buffers and are only dereferenced by the kernel during the
// recvmmsg call on the owning thread. The struct is never shared across
// threads (it is owned by its UdpTransport), so it is safe to move between
// threads.
unsafe impl Send for RecvBatchScratch {}

impl RecvBatchScratch {
    pub(crate) fn new() -> Self {
        Self {
            bufs: vec![vec![0u8; RECV_BATCH_BUF_SIZE]; RECV_BATCH_MAX],
            lens: vec![0; RECV_BATCH_MAX],
            addrs: vec![
                SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED), 0);
                RECV_BATCH_MAX
            ],
            truncated: 0,
            #[cfg(target_os = "linux")]
            iovs: Vec::with_capacity(RECV_BATCH_MAX),
            #[cfg(target_os = "linux")]
            addrs_raw: Vec::with_capacity(RECV_BATCH_MAX),
            #[cfg(target_os = "linux")]
            msgs: Vec::with_capacity(RECV_BATCH_MAX),
        }
    }
}

fn storage_to_socket_addr(storage: &libc::sockaddr_storage) -> Option<SocketAddr> {
    match storage.ss_family as libc::c_int {
        libc::AF_INET => {
            let sin = unsafe { *(storage as *const _ as *const libc::sockaddr_in) };
            Some(SocketAddr::from((
                std::net::Ipv4Addr::from(u32::from_be(sin.sin_addr.s_addr)),
                u16::from_be(sin.sin_port),
            )))
        }
        libc::AF_INET6 => {
            let sin6 = unsafe { *(storage as *const _ as *const libc::sockaddr_in6) };
            Some(SocketAddr::V6(std::net::SocketAddrV6::new(
                std::net::Ipv6Addr::from(sin6.sin6_addr.s6_addr),
                u16::from_be(sin6.sin6_port),
                sin6.sin6_flowinfo,
                sin6.sin6_scope_id,
            )))
        }
        _ => None,
    }
}

/// Receive up to `RECV_BATCH_MAX` datagrams in one call (or a `recv_from` loop on
/// platforms without `recvmmsg`). On success returns the number of datagrams
/// received; datagram `i` is in `scratch.bufs[i][..scratch.lens[i]]` from
/// `scratch.addrs[i]`. Returns `Ok(0)` when the socket is drained (WouldBlock).
pub(crate) fn recv_udp_batch(
    socket: &mut UdpSocket,
    scratch: &mut RecvBatchScratch,
) -> io::Result<usize> {
    #[cfg(target_os = "linux")]
    {
        // Rebuild the iovec/msghdr arrays each call; the backing Vecs keep their
        // capacity across calls so no allocation happens in steady state.
        scratch.iovs.clear();
        scratch.addrs_raw.clear();
        for i in 0..RECV_BATCH_MAX {
            let buf = &mut scratch.bufs[i];
            scratch.iovs.push(libc::iovec {
                iov_base: buf.as_mut_ptr() as *mut libc::c_void,
                iov_len: buf.len(),
            });
            scratch.addrs_raw.push(unsafe { std::mem::zeroed() });
        }
        scratch.msgs.clear();
        for i in 0..RECV_BATCH_MAX {
            scratch.msgs.push(libc::mmsghdr {
                msg_hdr: libc::msghdr {
                    msg_name: (&mut scratch.addrs_raw[i] as *mut libc::sockaddr_storage)
                        .cast::<libc::c_void>(),
                    msg_namelen: std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t,
                    msg_iov: &mut scratch.iovs[i],
                    msg_iovlen: 1,
                    msg_control: std::ptr::null_mut(),
                    msg_controllen: 0,
                    msg_flags: 0,
                },
                msg_len: 0,
            });
        }

        let fd = socket.as_raw_fd();
        let n = unsafe {
            libc::recvmmsg(
                fd,
                scratch.msgs.as_mut_ptr(),
                scratch.msgs.len() as u32,
                libc::MSG_DONTWAIT,
                std::ptr::null_mut(),
            )
        };
        if n >= 0 {
            let n = n as usize;
            for i in 0..n {
                // A datagram larger than the slot buffer is silently
                // truncated by the kernel (MSG_TRUNC): drop it rather than
                // process a corrupted frame.
                if scratch.msgs[i].msg_hdr.msg_flags & libc::MSG_TRUNC != 0 {
                    scratch.lens[i] = 0;
                    scratch.truncated += 1;
                    scratch.addrs[i] =
                        SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED), 0);
                    continue;
                }
                scratch.lens[i] = scratch.msgs[i].msg_len as usize;
                scratch.addrs[i] =
                    storage_to_socket_addr(&scratch.addrs_raw[i]).unwrap_or_else(|| {
                        SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED), 0)
                    });
            }
            // Slots beyond the received batch are stale from a previous
            // batch; zero them so no caller ever reads old datagram bytes.
            for i in n..RECV_BATCH_MAX {
                scratch.lens[i] = 0;
                scratch.addrs[i] =
                    SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED), 0);
            }
            return Ok(n);
        }
        let err = io::Error::last_os_error();
        match err.raw_os_error() {
            Some(libc::EAGAIN) => Ok(0), // EWOULDBLOCK == EAGAIN on Linux
            // Interrupted before any datagram: treat as drained; the next
            // poll wakeup will re-check the socket (level-triggered).
            Some(libc::EINTR) => Ok(0),
            // recvmmsg unsupported by socket family/kernel: fall back to the
            // portable recv_from loop.
            Some(libc::ENOSYS)
            | Some(libc::EINVAL)
            | Some(libc::EOPNOTSUPP)
            | Some(libc::EPERM) => recv_udp_batch_fallback(socket, scratch),
            _ => Err(err),
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        recv_udp_batch_fallback(socket, scratch)
    }
}

fn recv_udp_batch_fallback(
    socket: &mut UdpSocket,
    scratch: &mut RecvBatchScratch,
) -> io::Result<usize> {
    let mut n = 0usize;
    while n < RECV_BATCH_MAX {
        let (len, addr) = match socket.recv_from(&mut scratch.bufs[n]) {
            Ok((len, addr)) => (len, addr),
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
            Err(e) => return Err(e),
        };
        scratch.lens[n] = len;
        scratch.addrs[n] = addr;
        n += 1;
    }
    // Slots beyond the received batch are stale from a previous batch.
    for i in n..RECV_BATCH_MAX {
        scratch.lens[i] = 0;
        scratch.addrs[i] =
            SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED), 0);
    }
    Ok(n)
}
