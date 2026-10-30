//! The per-session task: one browser peer and the str0m pump that drives it.

use std::sync::Arc;
use std::time::Instant;

use tokio::net::UdpSocket;
use tokio::sync::mpsc;

use super::inbox::{SessionInbound, SessionNotice};
use super::session::{Outbound, PlaneChannel, RtcSession, SessionEvent};
use super::types::{RtcInput, UplinkEvent};

/// One browser peer, driven as its own task on the plane runtime. The loop is the
/// canonical str0m async pump: wait for the sooner of an inbound message or the
/// engine's next timeout, feed exactly one input, then flush all output.
pub(crate) async fn session_task(
    mut rtc: RtcSession,
    player_id: Arc<str>,
    mut inbox: mpsc::Receiver<SessionInbound>,
    socket: Arc<UdpSocket>,
    uplink_tx: mpsc::UnboundedSender<UplinkEvent>,
    notice_tx: mpsc::UnboundedSender<SessionNotice>,
) {
    loop {
        let delay = rtc.timeout_delay();
        let input = tokio::select! {
            biased;
            maybe = inbox.recv() => maybe,
            _ = tokio::time::sleep(delay) => {
                let out = rtc.feed_timeout(Instant::now());
                if pump_flush(&mut rtc, out, &socket, &uplink_tx, &notice_tx, &player_id).await {
                    break;
                }
                continue;
            }
        };

        let Some(inbound) = input else { break };
        let out = match inbound {
            SessionInbound::Datagram { from, bytes } => {
                let now = Instant::now();
                let bound = rtc.remote_addr() == Some(from);
                if bound || rtc.accepts(now, from, &bytes) {
                    rtc.feed_datagram(now, from, &bytes).unwrap_or_default()
                } else {
                    Vec::new()
                }
            }
            SessionInbound::Delta { seq, tick, body } => {
                // No open channel or an oversized body: this delta is stale, so drop it.
                rtc.send_delta(seq, tick, &body).unwrap_or_default()
            }
            SessionInbound::Reliable { body } => rtc.send_reliable(&body).unwrap_or_default(),
            SessionInbound::Answer { sdp } => rtc.accept_answer(&sdp).unwrap_or_default(),
            SessionInbound::Candidate { sdp } => rtc.add_remote_candidate(&sdp).unwrap_or_default(),
            SessionInbound::Close => break,
        };

        if pump_flush(&mut rtc, out, &socket, &uplink_tx, &notice_tx, &player_id).await {
            break;
        }
    }

    let _ = notice_tx.send(SessionNotice::Gone {
        session_id: rtc.session_id().to_string().into(),
    });
    let _ = uplink_tx.send(UplinkEvent::Disconnected { player_id });
}

/// Deliver the output of one engine drain: datagrams to the socket, channel data
/// to the uplink, events to the demux. Returns `true` if the session is closed and
/// the task should stop.
async fn pump_flush(
    rtc: &mut RtcSession,
    outbound: Vec<Outbound>,
    socket: &UdpSocket,
    uplink_tx: &mpsc::UnboundedSender<UplinkEvent>,
    notice_tx: &mpsc::UnboundedSender<SessionNotice>,
    player_id: &Arc<str>,
) -> bool {
    let session_id = rtc.session_id().clone();
    for out in outbound {
        match out {
            Outbound::Datagram {
                destination,
                contents,
            } => {
                // Fire-and-forget: a failed UDP send is an ICE glitch, not fatal.
                let _ = socket.send_to(&contents, destination).await;
            }
            Outbound::Data {
                channel, payload, ..
            } => match rtc.channel_role(channel) {
                Some(PlaneChannel::Unreliable) => {
                    let _ = uplink_tx.send(UplinkEvent::Input(RtcInput {
                        player_id: player_id.clone(),
                        seq: 0,
                        tick_hint: 0,
                        body: payload,
                    }));
                }
                Some(PlaneChannel::Reliable) | None => {
                    let _ = uplink_tx.send(UplinkEvent::Reliable {
                        player_id: player_id.clone(),
                        body: payload,
                    });
                }
            },
            Outbound::Event(event) => match event {
                SessionEvent::Connected => {
                    let _ = uplink_tx.send(UplinkEvent::Connected {
                        player_id: player_id.clone(),
                    });
                }
                SessionEvent::RemoteAddrBound { addr } => {
                    let _ = notice_tx.send(SessionNotice::RemoteAddrBound {
                        session_id: session_id.clone(),
                        addr,
                    });
                }
                SessionEvent::Closed => return true,
                SessionEvent::ChannelOpen { .. } | SessionEvent::ChannelClose { .. } => {}
            },
        }
    }
    false
}
