use axum::extract::ws::{Message, WebSocket};
use futures::StreamExt;

use crate::ApiState;
use crate::infra::seats::Seat;

use super::{SignalIn, SignalOut};
use super::SignalHub;
use super::inbound::{handle_text, send_frame};
use super::wire::ServerFrame;

struct CloseGuard {
    hub: SignalHub,
    seat: Seat,
    attach_id: u64,
}

impl Drop for CloseGuard {
    fn drop(&mut self) {
        let _ = self.hub.try_post(SignalIn::Close {
            seat: self.seat.clone(),
            attach_id: self.attach_id,
        });
    }
}

pub(crate) async fn run_socket(mut socket: WebSocket, state: ApiState, seat: Seat) {
    // A live route for this seat means another connection already holds the
    // token; refuse instead of silently hijacking its signal stream.
    let Some(mut replies) = state.signals.attach(&seat.id) else {
        let _ = send_frame(
            &mut socket,
            &ServerFrame::Error {
                message: "seat is already attached".into(),
            },
        )
        .await;
        return;
    };
    let _close_guard = CloseGuard {
        hub: state.signals.clone(),
        seat: seat.clone(),
        attach_id: replies.attach_id(),
    };

    let _ = state.signals.publish(
        &seat.id,
        SignalOut::IceServers {
            config: state.signals.ice_config().to_string(),
        },
    );

    loop {
        tokio::select! {
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
