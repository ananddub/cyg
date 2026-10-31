use axum::extract::ws::{Message, WebSocket};
use futures::StreamExt;

use crate::ApiState;
use crate::infra::seats::Seat;

use super::frames::{SignalIn, SignalOut};
use super::hub::SignalHub;
use super::inbound::{handle_text, send_frame};

struct CloseGuard {
    hub: SignalHub,
    seat: Seat,
}

impl Drop for CloseGuard {
    fn drop(&mut self) {
        let _ = self.hub.try_post(SignalIn::Close {
            seat: self.seat.clone(),
        });
    }
}

pub(crate) async fn run_socket(mut socket: WebSocket, state: ApiState, seat: Seat) {
    let mut replies = state.signals.attach(&seat.id);
    let _close_guard = CloseGuard {
        hub: state.signals.clone(),
        seat: seat.clone(),
    };

    let _ = state.signals.publish(
        &seat.id,
        SignalOut::IceServers {
            config: state.signals.ice_config().to_string(),
        },
    );

    loop {
        tokio::select! {
            biased;

            inbound = socket.next() => match inbound {
                Some(Ok(Message::Text(text))) => {
                    if !handle_text(&mut socket, &state, &seat, text.as_str()).await {
                        break;
                    }
                }
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                Some(Ok(_)) => {}
            },

            reply = replies.next() => match reply {
                Some(msg) => {
                    if send_frame(&mut socket, &msg.into()).await.is_err() {
                        break;
                    }
                }
                None => break,
            },
        }
    }
}
