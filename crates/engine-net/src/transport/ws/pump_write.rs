use super::protocol::close_code;
use std::io;
use std::io::Write;

use bytes::Buf;
use mio::Token;

use super::client::MAX_SEND_QUEUE;
use super::client::WsState;
use super::rfc6455::encode_wire_frame;
use super::transport::WsTransport;

impl WsTransport {
    pub(super) fn pump_broadcast(&mut self, colyseus_opcode: u8, payload: &[u8]) {
        self.sb.clear();
        self.sb.reserve(11 + payload.len());
        encode_wire_frame(colyseus_opcode, payload, &mut self.sb);
        let frame = &self.sb[..];
        let frame_len = frame.len();
        let mut queued = false;
        let mut overflowed = 0u64;
        for client in self.clients.values_mut() {
            if matches!(client.state, WsState::Connected) {
                client.state_frame_start = None;
                let cur_len = client.send_queue.len();
                if cur_len + frame_len > MAX_SEND_QUEUE {
                    overflowed += 1;
                    continue;
                }
                if client.send_queue.capacity().saturating_sub(cur_len) >= frame_len {
                    unsafe {
                        std::ptr::copy_nonoverlapping(
                            frame.as_ptr(),
                            client.send_queue.as_mut_ptr().add(cur_len),
                            frame_len,
                        );
                        client.send_queue.set_len(cur_len + frame_len);
                    }
                } else {
                    client.send_queue.extend_from_slice(frame);
                }
                queued = true;
            }
        }
        if queued {
            self.flush_dirty = true;
        }
        if overflowed > 0 {
            self.dropped_frames = self.dropped_frames.saturating_add(overflowed);
        }
    }

    pub(super) fn pump_flush(&mut self) -> io::Result<()> {
        self.flush_delayed_pings(std::time::Instant::now());

        self.flush_dirty = self.clients.values().any(|c| !c.send_queue.is_empty());
        if !self.flush_dirty {
            return Ok(());
        }
        let mut pending = false;
        // Reused scratch vecs: steady-state flush passes allocate nothing.
        self.bp_scratch.clear();
        self.write_failed_scratch.clear();
        for (token, client) in self.clients.iter_mut() {
            // One vectored syscall per client: inline queue + shared body +
            // post-shared tail offered together (writev batching).
            if !client.send_queue.is_empty() || client.shared_frame.is_some() {
                let before = client.send_queue.len();
                let r = client.write_out();
                if super::trace_enabled() {
                    eprintln!(
                        "[t] WO {:?} sid={} before={} after={} ok={}",
                        token,
                        client.session_id,
                        before,
                        client.send_queue.len(),
                        r.is_ok()
                    );
                }
                match r {
                    Ok(_) => {
                        client.state_frame_start = None;
                    }
                    Err(_) => {
                        self.write_failed_scratch.push(*token);
                    }
                }
            }
            pending |= !client.send_queue.is_empty() || client.shared_frame.is_some();

            if client.shared_frame.is_none() && !client.post_shared.is_empty() {
                // tail bytes queued behind a finished shared body — move them
                // into the inline queue so the next pass sends them.
                client.send_queue.unsplit(client.post_shared.split());
            }

            let want = !client.send_queue.is_empty() || client.shared_frame.is_some();
            if client.registered_writable != want {
                self.bp_scratch.push((*token, want));
            }
        }
        for i in 0..self.bp_scratch.len() {
            let (token, _want) = self.bp_scratch[i];
            if let Some(client) = self.clients.get_mut(&token) {
                crate::transport::ws::client_io::sync_writable_interest(client, &self.poll, token);
            }
        }
        for i in 0..self.write_failed_scratch.len() {
            let token = self.write_failed_scratch[i];
            // Socket gone or peer unreachable — same cleanup as a server
            // disconnect so the session does not linger holding buffers.
            if let Some(client) = self.clients.get(&token) {
                let client_id = client.session_id.to_string();
                let _ = self.disconnect_with(&client_id, close_code::SLOW_CONSUMER, "write failed");
            }
        }
        self.flush_dirty = pending;
        Ok(())
    }
}
