//! # Client Connection State
//!
//! Non-blocking socket state for each connected client:
//! - Read buffer with frame extraction
//! - Write buffer with batched write flushing
//! - Session tracking

use std::io::{self, Read, Write};
use mio::net::TcpStream;
use crate::framing::{decode_frame, encode_frame, OpCode};

pub struct ClientConn {
    pub stream: TcpStream,
    pub session_id: String,
    read_buf: Vec<u8>,
    write_buf: Vec<u8>,
}

impl ClientConn {
    pub fn new(stream: TcpStream, session_id: String) -> Self {
        Self {
            stream,
            session_id,
            read_buf: Vec::with_capacity(4096),
            write_buf: Vec::with_capacity(4096),
        }
    }

    /// Read incoming bytes from the non-blocking stream into internal buffer.
    /// Returns:
    /// - `Ok(bytes_read)`
    /// - `Ok(0)` indicates EOF / client disconnected cleanly.
    /// - `Err(e)` where e.kind() == WouldBlock means all available bytes have been read.
    pub fn do_read(&mut self) -> io::Result<usize> {
        let mut total = 0;
        let mut temp = [0u8; 4096];
        loop {
            match self.stream.read(&mut temp) {
                Ok(0) => {
                    // EOF
                    if total == 0 {
                        return Ok(0);
                    }
                    break;
                }
                Ok(n) => {
                    self.read_buf.extend_from_slice(&temp[..n]);
                    total += n;
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    break;
                }
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
        Ok(total)
    }

    /// Pull the next decoded frame from the read buffer if available.
    pub fn next_frame(&mut self) -> Option<(OpCode, Vec<u8>)> {
        decode_frame(&mut self.read_buf)
    }

    /// Queue a message into the outgoing write buffer for batched flush.
    pub fn queue_message(&mut self, opcode: OpCode, payload: &[u8]) {
        encode_frame(opcode, payload, &mut self.write_buf);
    }

    /// Attempt to flush pending bytes in `write_buf` out to the network socket.
    /// Returns `Ok(true)` if all pending bytes were successfully flushed, `Ok(false)` if partial/WouldBlock.
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
                    self.write_buf.drain(..n);
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

    /// Check if there is data pending in the outgoing write buffer.
    pub fn has_pending_writes(&self) -> bool {
        !self.write_buf.is_empty()
    }
}
