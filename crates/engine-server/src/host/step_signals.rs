use crate::host::*;
use engine_signaling::SignalIn;

impl<'a> Engine<'a> {
    pub(crate) fn pump_signals(&mut self) {
        let (hub_opt, rx_opt) = (self.signals.as_ref(), self.signal_rx.as_mut());
        let (hub, rx) = match (hub_opt, rx_opt) {
            (Some(h), Some(r)) => (h, r),
            _ => return,
        };
        while let Ok(sig) = rx.try_recv() {
            match sig {
                SignalIn::Offer { seat, sdp } => {
                    self.transport
                        .register_seat(&seat.id, &seat.room, &seat.options);
                    match self.transport.rtc_server().handle_offer(&seat.id, &sdp) {
                        Ok(answer) => {
                            hub.publish(
                                &seat.id,
                                engine_signaling::SignalOut::Answer { sdp: answer },
                            );
                            tracing::info!(
                                "[server] ⚡ WebRTC offer accepted for seat {} (room '{}'), SDP answer returned",
                                seat.id,
                                seat.room
                            );
                        }
                        Err(e) => {
                            hub.publish(
                                &seat.id,
                                engine_signaling::SignalOut::Error { message: e },
                            );
                        }
                    }
                }
                SignalIn::Candidate {
                    seat, candidate, ..
                } => {
                    if let Err(e) = self
                        .transport
                        .rtc_server()
                        .handle_candidate(&seat.id, &candidate)
                    {
                        tracing::error!(
                            "[server] ⚠️ seat {} candidate error: {e} ({candidate})",
                            seat.id
                        );
                        hub.publish(
                            &seat.id,
                            engine_signaling::SignalOut::Error {
                                message: format!("candidate rejected: {e}"),
                            },
                        );
                    }
                }
                SignalIn::Close { seat, attach_id } => {
                    if !hub.is_attached(&seat.id, attach_id) {
                        tracing::debug!(
                            "[server] ignoring stale close for seat {} (attach {attach_id} no longer current)",
                            seat.id
                        );
                        continue;
                    }
                    tracing::info!("[server] signaling socket closed for seat {}", seat.id);
                    let _ =
                        self.transport
                            .disconnect_with(&seat.id, 1000, "signaling socket closed");
                }
            }
        }
    }
}
