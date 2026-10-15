//! Server-side heartbeat.

use std::time::{Duration, Instant};

use super::client::WsState;
use super::rfc6455::send_ws_ping;
use super::transport::WsTransport;

/// Server-side heartbeat (mirrors Colyseus `autoTerminateUnresponsiveClients`):
/// WS ping frames are sent to idle clients; a client that misses more than
/// `max_retries` pongs is terminated. Browsers answer WS pings in the network
/// stack, so real SDK clients are safe even in throttled background tabs.
#[derive(Debug, Clone, Copy)]
pub struct HeartbeatConfig {
    pub enabled: bool,
    pub ping_interval: Duration,
    pub max_retries: u32,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        // Colyseus ws-transport defaults: pingInterval 3000ms, pingMaxRetries 2.
        Self {
            enabled: true,
            ping_interval: Duration::from_millis(3000),
            max_retries: 2,
        }
    }
}

impl WsTransport {
    /// Send WS ping frames to idle clients and terminate the unresponsive
    /// ones. Killed tokens are collected in `self.hb_kills` (reused across
    /// polls, no allocation).
    pub(super) fn run_heartbeat(&mut self, now: Instant) {
        let (hb, clients) = (&mut self.hb_kills, &mut self.clients);
        hb.clear();
        if !self.heartbeat.enabled {
            return;
        }
        for (token, client) in clients.iter_mut() {
            if client.state != WsState::Connected {
                continue;
            }
            match client.ping_sent_at {
                Some(sent_at) => {
                    if now.duration_since(sent_at) >= self.heartbeat.ping_interval {
                        client.missed_pings += 1;
                        if client.missed_pings > self.heartbeat.max_retries {
                            hb.push(*token);
                            continue;
                        }
                        send_ws_ping(client);
                        client.ping_sent_at = Some(now);
                    }
                }
                None => {
                    if now.duration_since(client.last_activity) >= self.heartbeat.ping_interval {
                        send_ws_ping(client);
                        client.ping_sent_at = Some(now);
                    }
                }
            }
        }
    }
}
