//! The str0m pump: feed engine inputs and drain its outputs.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use str0m::net::{Protocol, Receive};
use str0m::{Event, Input, Output};

use super::engine::RtcSession;
use super::types::{Outbound, SessionEvent};
use super::{LABEL_RELIABLE, LABEL_UNRELIABLE};
use crate::rtc_plane::RtcPlaneError;

impl RtcSession {
    /// Does this datagram belong to us? Cheap ICE/STUN affinity check.
    pub fn accepts(&self, now: Instant, from: SocketAddr, payload: &[u8]) -> bool {
        match Receive::new(Protocol::Udp, from, self.local_addr, payload) {
            Ok(recv) => self.rtc.accepts(&Input::Receive(now, recv)),
            Err(_) => false,
        }
    }

    /// Ingest one inbound UDP datagram. Call only when [`Self::accepts`] agreed,
    /// or once the peer address is already bound.
    pub fn feed_datagram(
        &mut self,
        now: Instant,
        from: SocketAddr,
        payload: &[u8],
    ) -> Result<Vec<Outbound>, RtcPlaneError> {
        let recv = Receive::new(Protocol::Udp, from, self.local_addr, payload)
            .map_err(|e| RtcPlaneError::Transport(format!("receive build: {e:?}")))?;
        let first_binding = self.remote_addr.is_none();
        self.rtc
            .handle_input(Input::Receive(now, recv))
            .map_err(|e| RtcPlaneError::Transport(format!("handle_input: {e:?}")))?;
        if self.remote_addr.is_none() {
            self.remote_addr = Some(from);
        }
        let mut out = self.drain();
        if first_binding && self.remote_addr.is_some() {
            out.push(Outbound::Event(SessionEvent::RemoteAddrBound { addr: from }));
        }
        Ok(out)
    }

    /// Feed a timeout. Call when `now >= poll_timeout()` and no datagram arrived.
    pub fn feed_timeout(&mut self, now: Instant) -> Vec<Outbound> {
        let _ = self.rtc.handle_input(Input::Timeout(now));
        self.drain()
    }

    /// The instant this session next wants a timeout input.
    pub fn poll_timeout(&self) -> Instant {
        self.next_timeout
    }

    /// Convert a str0m `Instant` timeout into a tokio-friendly `Duration` from now,
    /// saturating to zero if it already elapsed (the caller then pumps immediately).
    pub fn timeout_delay(&self) -> Duration {
        self.next_timeout.saturating_duration_since(Instant::now())
    }

    /// Drain all queued engine output to the next timeout, mapping it to
    /// [`Outbound`] and updating session state as side effects.
    pub(super) fn drain(&mut self) -> Vec<Outbound> {
        let mut out = Vec::new();
        loop {
            match self.rtc.poll_output() {
                Ok(Output::Transmit(t)) => {
                    out.push(Outbound::Datagram {
                        destination: t.destination,
                        contents: t.contents.to_vec(),
                    });
                }
                Ok(Output::Event(event)) => {
                    if let Some(mapped) = self.map_event(event) {
                        out.extend(mapped);
                    }
                }
                Ok(Output::Timeout(now)) => {
                    self.next_timeout = now;
                    break;
                }
                Err(_) => {
                    // Engine is dead (disconnect / protocol error). Stop driving.
                    self.next_timeout = Instant::now() + Duration::from_secs(3600);
                    out.push(Outbound::Event(SessionEvent::Closed));
                    break;
                }
            }
        }
        out
    }

    fn map_event(&mut self, event: Event) -> Option<Vec<Outbound>> {
        match event {
            Event::Connected => {
                self.connected = true;
                Some(vec![Outbound::Event(SessionEvent::Connected)])
            }
            Event::ChannelOpen(id, label) => {
                self.channel_labels.insert(id, label.clone());
                if label == LABEL_UNRELIABLE {
                    self.ch_unreliable = Some(id);
                } else if label == LABEL_RELIABLE {
                    self.ch_reliable = Some(id);
                }
                Some(vec![Outbound::Event(SessionEvent::ChannelOpen { id, label })])
            }
            Event::ChannelClose(id) => {
                if self.ch_unreliable == Some(id) {
                    self.ch_unreliable = None;
                }
                if self.ch_reliable == Some(id) {
                    self.ch_reliable = None;
                }
                Some(vec![Outbound::Event(SessionEvent::ChannelClose { id })])
            }
            Event::ChannelData(data) => Some(vec![Outbound::Data {
                channel: data.id,
                binary: data.binary,
                payload: data.data,
            }]),
            Event::Closed => Some(vec![Outbound::Event(SessionEvent::Closed)]),
            _ => None,
        }
    }
}
