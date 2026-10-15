//! The socket task: attach, ICE handout, and the two-way relay loop.

use axum::extract::ws::{Message, WebSocket};
use futures::StreamExt;

use crate::ApiState;
use crate::seats::Seat;

use super::frames::{SignalIn, SignalOut};
use super::hub::SignalHub;
use super::inbound::{handle_text, send_frame};

/// Posts exactly one [`SignalIn::Close`] for its socket, however that socket's
/// task ends: a client close, a WS Close frame, an error, or a cancelled
/// future. `try_post` is a non-blocking send, which makes a `Drop`-time
/// notice legal on this plane.
struct CloseGuard {
    hub: SignalHub,
    seat: Seat,
}

impl Drop for CloseGuard {
    fn drop(&mut self) {
        let _ = self.hub.try_post(SignalIn::Close { seat: self.seat.clone() });
    }
}

pub(super) async fn run_socket(mut socket: WebSocket, state: ApiState, seat: Seat) {
    let mut replies = state.signals.attach(&seat.id);
    // Declared after `replies`, so it drops first: the data plane sees the
    // seat close while its reply route is still addressable, and the route
    // removal that follows is generation-checked.
    let _close_guard = CloseGuard { hub: state.signals.clone(), seat: seat.clone() };

    // The client cannot gather relay candidates without the ICE list, so this
    // plane publishes it to the seat — through the same hub the data plane
    // answers on — before anything else can reach the socket. The queue is
    // freshly attached and empty, so this publish cannot fail while owned.
    let _ = state.signals.publish(
        &seat.id,
        SignalOut::IceServers { config: state.signals.ice_config().to_string() },
    );

    loop {
        tokio::select! {
            // biased: an inbound offer must not queue behind a burst of our
            // own candidates when both are ready in the same tick.
            biased;

            inbound = socket.next() => match inbound {
                Some(Ok(Message::Text(text))) => {
                    if !handle_text(&mut socket, &state, &seat, text.as_str()).await {
                        break;
                    }
                }
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                // Ping/Pong are answered by axum; binary frames are not part
                // of this protocol and are ignored rather than misparsed.
                Some(Ok(_)) => {}
            },

            reply = replies.next() => match reply {
                Some(msg) => {
                    if send_frame(&mut socket, &msg.into()).await.is_err() {
                        break;
                    }
                }
                // Every sender is gone, which for a live socket means the hub
                // was dropped with the plane shutting down.
                None => break,
            },
        }
    }
}
