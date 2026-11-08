use std::io::{self, Read};
use std::net::SocketAddr;

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
