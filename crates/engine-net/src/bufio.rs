//! Direct-to-buffer socket I/O.
//!
//! The kernel writes straight into a `BytesMut`'s spare capacity, so received
//! bytes are never copied through an intermediate stack buffer and never
//! zero-filled. Frames are then sliced out as zero-copy `Bytes` views.

use std::io::{self, Read};
use std::net::SocketAddr;

use bytes::BytesMut;
use mio::net::{TcpStream, UdpSocket};

/// One non-blocking TCP read directly into `dst`, committing exactly the
/// bytes the kernel initialized. `Ok(0)` means EOF.
pub(crate) fn read_tcp(stream: &mut TcpStream, dst: &mut BytesMut, want: usize) -> io::Result<usize> {
    dst.reserve(want);
    let spare = dst.spare_capacity_mut();
    let len = spare.len().min(want);
    // SAFETY: `MaybeUninit<u8>` and `u8` share layout and alignment, and `read` only writes into the slice it is handed.
    let buf = unsafe { std::slice::from_raw_parts_mut(spare.as_mut_ptr() as *mut u8, len) };
    match stream.read(buf) {
        Ok(0) => Ok(0),
        Ok(n) => {
            // SAFETY: the kernel initialized exactly `buf[..n]`, and `old_len + n <= old_len + spare.len() <= capacity`.
            unsafe { dst.set_len(dst.len() + n) };
            Ok(n)
        }
        Err(e) => Err(e),
    }
}

/// One non-blocking UDP `recv_from` directly into `dst`.
/// `Ok(None)` means the socket would block (all datagrams drained).
pub(crate) fn recv_udp(
    socket: &mut UdpSocket,
    dst: &mut BytesMut,
    want: usize,
) -> io::Result<Option<(usize, SocketAddr)>> {
    dst.reserve(want);
    let spare = dst.spare_capacity_mut();
    let len = spare.len().min(want);
    // SAFETY: as in `read_tcp` — the kernel only writes into `buf`.
    let buf = unsafe { std::slice::from_raw_parts_mut(spare.as_mut_ptr() as *mut u8, len) };
    match socket.recv_from(buf) {
        Ok((n, addr)) => {
            // SAFETY: the kernel initialized exactly `buf[..n]`.
            unsafe { dst.set_len(dst.len() + n) };
            Ok(Some((n, addr)))
        }
        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
        Err(e) => Err(e),
    }
}
