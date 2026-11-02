use std::io;
use std::net::SocketAddr;

const SENDMMSG_CHUNK: usize = 1024;

pub trait BatchSend {
    fn send_one(&self, data: &[u8], dest: SocketAddr) -> std::io::Result<()>;
}

impl BatchSend for std::net::UdpSocket {
    fn send_one(&self, data: &[u8], dest: SocketAddr) -> std::io::Result<()> {
        self.send_to(data, dest).map(|_| ())
    }
}

impl BatchSend for mio::net::UdpSocket {
    fn send_one(&self, data: &[u8], dest: SocketAddr) -> std::io::Result<()> {
        self.send_to(data, dest).map(|_| ())
    }
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
pub fn send_datagram_batch<S: BatchSend, B: AsRef<[u8]>>(
    socket: &S,
    msgs: &[(SocketAddr, B)],
) -> usize {
    let mut sent = 0;
    for (dest, bytes) in msgs {
        if socket.send_one(bytes.as_ref(), *dest).is_ok() {
            sent += 1;
        }
    }
    sent
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
pub fn send_datagram_batch<F: std::os::unix::io::AsFd, B: AsRef<[u8]>>(
    socket: &F,
    msgs: &[(SocketAddr, B)],
) -> usize {
    if msgs.is_empty() {
        return 0;
    }
    let fd = socket.as_fd();
    #[allow(clippy::needless_borrow)]
    let fd = std::os::unix::io::AsRawFd::as_raw_fd(&fd);
    let mut sent_total = 0usize;
    let mut chunk_start = 0usize;

    while chunk_start < msgs.len() {
        let chunk = &msgs[chunk_start..(chunk_start + SENDMMSG_CHUNK).min(msgs.len())];
        let mut msgvec: Vec<libc::mmsghdr> = Vec::with_capacity(chunk.len());
        let mut iovecs: Vec<libc::iovec> = Vec::with_capacity(chunk.len());
        let mut addrs: Vec<(libc::sockaddr_storage, libc::socklen_t)> =
            Vec::with_capacity(chunk.len());

        for (addr, data) in chunk {
            let (storage, len) = crate::transport::socket::sockaddr_storage_of(addr);
            addrs.push((storage, len));
            let slice = data.as_ref();
            iovecs.push(libc::iovec {
                iov_base: slice.as_ptr() as *mut libc::c_void,
                iov_len: slice.len(),
            });
        }
        for i in 0..chunk.len() {
            msgvec.push(libc::mmsghdr {
                msg_hdr: libc::msghdr {
                    msg_name: &mut addrs[i].0 as *mut libc::sockaddr_storage as *mut libc::c_void,
                    msg_namelen: addrs[i].1,
                    msg_iov: &mut iovecs[i],
                    msg_iovlen: 1,
                    msg_control: std::ptr::null_mut(),
                    msg_controllen: 0,
                    msg_flags: 0,
                },
                msg_len: 0,
            });
        }

        let sent = unsafe { libc::sendmmsg(fd, msgvec.as_mut_ptr(), msgvec.len() as u32, 0) };
        if sent < 0 {
            for (addr, data) in chunk {
                let _ = socket_send_fallback(socket, addr, data.as_ref());
                sent_total += 1;
            }
            chunk_start += chunk.len();
            continue;
        }
        sent_total += sent as usize;

        if (sent as usize) < chunk.len() {
            for (addr, data) in chunk.iter().skip(sent as usize) {
                let _ = socket_send_fallback(socket, addr, data.as_ref());
                sent_total += 1;
            }
        }
        chunk_start += chunk.len();
    }
    sent_total
}

#[cfg(unix)]
fn socket_send_fallback<F: std::os::unix::io::AsFd>(
    socket: &F,
    addr: &SocketAddr,
    data: &[u8],
) -> io::Result<usize> {
    socket2::SockRef::from(socket).send_to(data, &socket2::SockAddr::from(*addr))
}
