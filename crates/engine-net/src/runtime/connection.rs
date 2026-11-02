
use crate::framing::{OpCode, encode_frame, take_frame};
use bytes::{Buf, Bytes, BytesMut};
use mio::net::TcpStream;
use std::io::{self, Write};
use std::sync::Arc;

pub struct ClientConn {
    pub stream: TcpStream,
    pub session_id: Arc<str>,
    read_buf: BytesMut,
    write_buf: BytesMut,
}

impl ClientConn {
    pub fn new(stream: TcpStream, session_id: Arc<str>) -> Self {
        Self {
            stream,
            session_id,
            read_buf: BytesMut::with_capacity(4096),
            write_buf: BytesMut::with_capacity(4096),
        }
    }

    pub fn do_read(&mut self) -> io::Result<usize> {
        let mut total = 0;
        loop {
            match crate::bufio::read_tcp(&mut self.stream, &mut self.read_buf, 16 * 1024) {
                Ok(0) => {

                    if total == 0 {
                        return Ok(0);
                    }
                    break;
                }
                Ok(n) => total += n,
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(total)
    }

    pub fn next_frame(&mut self) -> Option<(OpCode, Bytes)> {
        take_frame(&mut self.read_buf)
    }

    pub fn queue_message(&mut self, opcode: OpCode, payload: &[u8]) {
        encode_frame(opcode, payload, &mut self.write_buf);
    }

    pub fn do_write(&mut self) -> io::Result<bool> {
        while !self.write_buf.is_empty() {
            match self.stream.write(&self.write_buf) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "Zero bytes written to socket",
                    ));
                }
                Ok(n) => {
                    self.write_buf.advance(n);
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    return Ok(false);
                }
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
        Ok(true)
    }

    pub fn has_pending_writes(&self) -> bool {
        !self.write_buf.is_empty()
    }
}
