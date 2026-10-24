//! Translating one session's `str0m` output into transport events, classified
//! inbound frames, datagrams and contract counters.

use str0m::{Event, Output};

use super::contract::{
    Inbound, LabelVerdict, Lane, RejectReason, classify_inbound, classify_label, contract_mismatch,
};
use super::session::{Sink, WebRtcSession};
use super::types::WebRtcServerEvent;
use super::WebRtcServer;

impl WebRtcServer {
    /// Drain this session's `str0m` output queue.
    ///
    /// A `Channel::write` only lands in the SCTP association; the datagram
    /// exists once the output queue is polled. Skipping this makes every delta
    /// leave a tick late, and a host that sends then asks for transmits in the
    /// same iteration would get nothing back.
    pub(super) fn settle(session: &mut WebRtcSession, sink: &mut Sink) {
        Self::poll_session_events(session, sink);
    }

    pub(super) fn poll_session_events(session: &mut WebRtcSession, sink: &mut Sink) {
        while let Ok(output) = session.rtc.poll_output() {
            match output {
                Output::Transmit(send) => {
                    sink.pending.push((send.destination, send.contents.to_vec()));
                }
                Output::Event(Event::Connected) => {
                    session.connected = true;
                    sink.events.push(WebRtcServerEvent::Connected {
                        client_id: session.client_id.clone(),
                    });
                }
                Output::Event(Event::ChannelOpen(cid, label)) => {
                    let verdict = classify_label(&label);
                    let legacy = matches!(verdict, LabelVerdict::LegacyAlias(_));
                    if let LabelVerdict::Contract(lane) | LabelVerdict::LegacyAlias(lane) = verdict {
                        match lane {
                            Lane::Unreliable => session.ch_unreliable = Some(cid),
                            Lane::Reliable => session.ch_reliable = Some(cid),
                        }
                        session.lanes.insert(cid, lane);
                        if legacy {
                            sink.stats.legacy_alias_channels += 1;
                            if !session.legacy_alias_warned {
                                session.legacy_alias_warned = true;
                                eprintln!(
                                    "[webrtc] ⚠️ client {} opened '{label}': not a contract label, mapped to {} as a legacy alias",
                                    session.client_id,
                                    lane.label()
                                );
                            }
                        }
                        // The label names a lane; the negotiated SCTP parameters decide what it is, so record a disagreement rather than trust either alone.
                        let mismatch = session
                            .rtc
                            .channel(cid)
                            .and_then(|c| c.config().cloned())
                            .and_then(|config| contract_mismatch(lane, &config));
                        if let Some(reason) = mismatch {
                            sink.stats.contract_violations += 1;
                            eprintln!("[webrtc] ⚠️ channel '{label}' on {}: {reason}", session.client_id);
                        }
                    }
                    // Anything else stays unmapped: neither writable as a game lane nor believable as a source of game frames.
                    sink.events.push(WebRtcServerEvent::ChannelOpen {
                        client_id: session.client_id.clone(),
                        label,
                    });
                }
                Output::Event(Event::ChannelData(data)) => {
                    let lane = session.lanes.get(&data.id).copied();
                    let mut inbound = classify_inbound(lane, &data.data);
                    if let Inbound::Input { seq, ref mut resupplied, .. } = inbound {
                        *resupplied = !session.last_input_seq.advance(seq);
                        sink.stats.inputs_received += 1;
                        if *resupplied {
                            sink.stats.inputs_stale += 1;
                        }
                    } else if let Inbound::Ack(ack) = inbound {
                        if session.acked_event_seq.advance(ack.seq) {
                            sink.stats.acks_received += 1;
                        } else {
                            sink.stats.acks_stale += 1;
                        }
                    } else if matches!(inbound, Inbound::Event { .. }) {
                        sink.stats.events_received += 1;
                    }

                    match inbound {
                        Inbound::Refused(reason) => match reason {
                            RejectReason::WrongLane { .. } => sink.stats.frames_misrouted += 1,
                            RejectReason::Malformed { .. } => sink.stats.frames_malformed += 1,
                            RejectReason::OffContract => sink.stats.frames_off_contract += 1,
                        },
                        other => {
                            let label = session
                                .channel_labels
                                .get(&data.id)
                                .cloned()
                                .unwrap_or_else(|| lane.map(Lane::label).unwrap_or("unknown").to_string());
                            sink.inbound.push((session.client_id.clone(), other));
                            sink.events.push(WebRtcServerEvent::Message {
                                client_id: session.client_id.clone(),
                                channel_label: label,
                                data: data.data,
                            });
                        }
                    }
                }
                Output::Event(Event::ChannelClose(cid)) => {
                    let lane = session.lanes.remove(&cid);
                    if session.ch_unreliable == Some(cid) {
                        session.ch_unreliable = None;
                    }
                    if session.ch_reliable == Some(cid) {
                        session.ch_reliable = None;
                    }
                    if let Some(lane) = lane {
                        // Whatever was waiting for it can never be delivered.
                        let n = session.queue(lane).clear();
                        sink.stats.frames_dropped_no_channel += n as u64;
                    }
                    session.channel_labels.remove(&cid);
                    if session.ch_reliable.is_none() && session.ch_unreliable.is_none() && session.connected {
                        sink.events.push(WebRtcServerEvent::Disconnected {
                            client_id: session.client_id.clone(),
                        });
                    }
                }
                Output::Timeout(t) => {
                    session.next_timeout = Some(t);
                    break;
                }
                _ => {}
            }
        }
    }
}
