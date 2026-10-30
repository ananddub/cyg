//! One iteration of the worker event loop: accept, read, write, tick, flush.
//!
//! The event pass runs as free functions over destructured fields: `Events::iter`
//! borrows `events` for the whole pass, so a `&mut self` method called from
//! inside it would overlap that borrow. Disjoint field borrows do not.

use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use mio::event::Event;
use mio::net::TcpListener;
use mio::{Interest, Poll, Token};

use super::{ClientPacket, LISTENER_TOKEN, MioWorker, WAKER_TOKEN};
use crate::runtime::connection::ClientConn;
use engine_core::StepContext;

impl MioWorker {
    /// Run a single step/iteration of the worker event loop.
    /// Received packets are appended into the caller-owned `packets` vec,
    /// so the hot loop performs zero per-step Vec allocations.
    pub fn step<F>(
        &mut self,
        max_wait: Duration,
        packets: &mut Vec<ClientPacket>,
        mut on_tick: F,
    ) -> io::Result<()>
    where
        F: FnMut(StepContext, &mut HashMap<Token, ClientConn>),
    {
        packets.clear();
        let timeout = self.tick_budget(Instant::now(), max_wait);
        self.poll.poll(&mut self.events, Some(timeout))?;

        let MioWorker {
            poll,
            events,
            listener,
            clients,
            next_token,
            disconnected_tokens,
            ..
        } = self;
        let mut disconnected = std::mem::take(disconnected_tokens);
        disconnected.clear();

        for event in events.iter() {
            match event.token() {
                LISTENER_TOKEN => accept_pending(poll, listener, clients, next_token),
                WAKER_TOKEN => {}
                client_token => {
                    if service_client(clients, client_token, event, packets) {
                        disconnected.push(client_token);
                    }
                }
            }
        }

        for token in disconnected.drain(..) {
            if let Some(mut conn) = clients.remove(&token) {
                let _ = poll.registry().deregister(&mut conn.stream);
            }
        }
        *disconnected_tokens = disconnected;

        let now = Instant::now();
        if now >= self.next_tick_deadline {
            self.run_tick(now, &mut on_tick);
        }

        self.stats.ccu.store(self.clients.len(), Ordering::Relaxed);
        if !packets.is_empty() {
            self.stats
                .total_packets_in
                .fetch_add(packets.len() as u64, Ordering::Relaxed);
        }
        Ok(())
    }

    /// Advance the fixed clock, hand the tick to the game, then batch-flush
    /// every connection's write buffer in one pass.
    fn run_tick<F>(&mut self, now: Instant, on_tick: &mut F)
    where
        F: FnMut(StepContext, &mut HashMap<Token, ClientConn>),
    {
        let tick_start = Instant::now();
        let ctx = self.clock.advance();
        self.next_tick_deadline += self.clock.step_duration;
        if self.next_tick_deadline < now {
            self.next_tick_deadline = now + self.clock.step_duration;
        }
        on_tick(ctx, &mut self.clients);

        for (_, conn) in self.clients.iter_mut() {
            if conn.has_pending_writes() {
                let _ = conn.do_write();
            }
        }

        let elapsed_us = tick_start.elapsed().as_micros() as u64;
        self.stats
            .last_tick_time_us
            .store(elapsed_us, Ordering::Relaxed);
        self.stats.total_ticks.fetch_add(1, Ordering::Relaxed);
    }
}

/// Accept every connection the kernel has queued on this worker's
/// `SO_REUSEPORT` listener, registering each with a fresh token.
fn accept_pending(
    poll: &mut Poll,
    listener: &mut TcpListener,
    clients: &mut HashMap<Token, ClientConn>,
    next_token: &mut usize,
) {
    loop {
        match listener.accept() {
            Ok((mut stream, peer_addr)) => {
                let token = Token(*next_token);
                *next_token += 1;

                if let Err(e) = stream.set_nodelay(true) {
                    tracing::error!("Failed to set nodelay: {}", e);
                }

                if let Err(e) = poll.registry().register(
                    &mut stream,
                    token,
                    Interest::READABLE | Interest::WRITABLE,
                ) {
                    tracing::error!("Failed to register client: {}", e);
                    continue;
                }

                // Allocated exactly once per connection; every packet referencing it is a refcount bump.
                let session_id: Arc<str> = format!("sess_{}_{}", token.0, peer_addr.port()).into();
                clients.insert(token, ClientConn::new(stream, session_id));
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
            Err(e) => {
                tracing::error!("Accept error: {}", e);
                break;
            }
        }
    }
}

/// Read and write one connection. Returns `true` when it must be dropped.
fn service_client(
    clients: &mut HashMap<Token, ClientConn>,
    client_token: Token,
    event: &Event,
    packets: &mut Vec<ClientPacket>,
) -> bool {
    let Some(conn) = clients.get_mut(&client_token) else {
        return false;
    };
    let mut conn_disconnected = false;

    if event.is_readable() {
        match conn.do_read() {
            Ok(0) => conn_disconnected = true,
            Ok(_) => {
                while let Some((opcode, payload)) = conn.next_frame() {
                    packets.push(ClientPacket {
                        session_id: conn.session_id.clone(),
                        token: client_token,
                        opcode,
                        payload,
                    });
                }
            }
            Err(e) => {
                tracing::error!("Socket read error: {}", e);
                conn_disconnected = true;
            }
        }
    }

    if event.is_writable() && conn.has_pending_writes() {
        if let Err(e) = conn.do_write() {
            tracing::error!("Socket write error: {}", e);
            conn_disconnected = true;
        }
    }
    conn_disconnected
}
