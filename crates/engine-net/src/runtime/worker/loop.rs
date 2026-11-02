use std::collections::HashMap;
use std::io;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use mio::Token;

use super::{ClientPacket, LISTENER_TOKEN, MioWorker, WAKER_TOKEN, net};
use crate::runtime::connection::ClientConn;
use engine_core::StepContext;

impl MioWorker {

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
                LISTENER_TOKEN => net::accept_pending(poll, listener, clients, next_token),
                WAKER_TOKEN => {}
                client_token => {
                    if net::service_client(clients, client_token, event, packets) {
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
