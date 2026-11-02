use std::io;
use std::net::SocketAddr;

#[cfg(unix)]
const SOL_UDP: libc::c_int = 17;
#[cfg(unix)]
const UDP_SEGMENT: libc::c_int = 103;

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
pub fn send_gso<S: super::batch::BatchSend>(
    _socket: &S,
    _addr: &SocketAddr,
    _segments: &[&[u8]],
    _seg_size: usize,
) -> Result<(), std::io::Error> {
    Err(std::io::Error::other("GSO is glibc-linux-only"))
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
pub fn gso_run_len(msgs: &[(SocketAddr, impl AsRef<[u8]>)], start: usize) -> usize {
    let (dest, first) = &msgs[start];
    let seg = first.as_ref().len();
    if seg == 0 {
        return 1;
    }
    let mut n = 1;
    while start + n < msgs.len() {
        let (next_dest, next) = &msgs[start + n];
        if *next_dest != *dest || next.as_ref().len() != seg {
            break;
        }
        n += 1;
    }
    n
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
pub fn send_gso<F: std::os::unix::io::AsFd>(
    socket: &F,
    addr: &SocketAddr,
    segments: &[&[u8]],
    seg_size: usize,
) -> io::Result<()> {
    debug_assert!(seg_size > 0);
    debug_assert!(!segments.is_empty());

    let fd = socket.as_fd();
    #[allow(clippy::needless_borrow)]
    let fd = std::os::unix::io::AsRawFd::as_raw_fd(&fd);

    let iovecs: Vec<libc::iovec> = segments
        .iter()
        .map(|s| libc::iovec {
            iov_base: s.as_ptr() as *mut libc::c_void,
            iov_len: s.len(),
        })
        .collect();

    let cmsg_space = unsafe { libc::CMSG_SPACE(std::mem::size_of::<u16>() as u32) as usize };
    let mut control = vec![0u8; cmsg_space];
    unsafe {
        let cmsg = libc::CMSG_FIRSTHDR(&libc::msghdr {
            msg_control: control.as_mut_ptr() as *mut libc::c_void,
            msg_controllen: cmsg_space,
            ..std::mem::zeroed()
        });
        (*cmsg).cmsg_level = SOL_UDP;
        (*cmsg).cmsg_type = UDP_SEGMENT;
        (*cmsg).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<u16>() as u32) as usize;
        std::ptr::write_unaligned(libc::CMSG_DATA(cmsg) as *mut u16, seg_size as u16);
    }

    let (storage, namelen) = crate::transport::socket::sockaddr_storage_of(addr);
    let msghdr = libc::msghdr {
        msg_name: &storage as *const libc::sockaddr_storage as *mut libc::c_void,
        msg_namelen: namelen,
        msg_iov: iovecs.as_ptr() as *mut libc::iovec,
        msg_iovlen: iovecs.len(),
        msg_control: control.as_mut_ptr() as *mut libc::c_void,
        msg_controllen: cmsg_space,
        msg_flags: 0,
    };

    let sent = unsafe { libc::sendmsg(fd, &msghdr, 0) };
    if sent < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
pub fn gso_run_len(msgs: &[(SocketAddr, impl AsRef<[u8]>)], start: usize) -> usize {
    let (dest, first) = &msgs[start];
    let seg = first.as_ref().len();
    if seg == 0 {
        return 1;
    }
    let mut n = 1;
    while start + n < msgs.len() {
        let (addr, data) = &msgs[start + n];
        if addr != dest {
            break;
        }
        let len = data.as_ref().len();
        if len != seg && start + n + 1 != msgs.len() && len > seg {
            break;
        }
        n += 1;
    }
    n
}
