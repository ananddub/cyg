use axum::extract::ws::{Message, WebSocket};

use crate::ApiState;
use crate::infra::seats::Seat;

use super::frames::SignalIn;
use super::wire::{ClientFrame, ServerFrame};

pub(super) async fn handle_text(
    socket: &mut WebSocket,
    state: &ApiState,
    seat: &Seat,
    text: &str,
) -> bool {
    match serde_json::from_str::<ClientFrame>(text) {
        Ok(ClientFrame::Offer { sdp }) => {
            if !state.signals.post(SignalIn::Offer {
                seat: seat.clone(),
                sdp,
            }) {
                reject(socket).await;
            }
            true
        }
        Ok(ClientFrame::Candidate {
            candidate,
            sdp_mid,
            sdp_index,
        }) => {
            if !state.signals.post(SignalIn::Candidate {
                seat: seat.clone(),
                candidate,
                sdp_mid,
                sdp_index,
            }) {
                reject(socket).await;
            }
            true
        }
        Ok(ClientFrame::Close) => false,
        Err(e) => {
            let _ = send_frame(
                socket,
                &ServerFrame::Error {
                    message: format!("malformed signaling frame: {e}"),
                },
            )
            .await;
            true
        }
    }
}

async fn reject(socket: &mut WebSocket) {
    let _ = send_frame(
        socket,
        &ServerFrame::Error {
            message: "data plane is not accepting signaling".into(),
        },
    )
    .await;
}

pub(super) async fn send_frame(
    socket: &mut WebSocket,
    frame: &ServerFrame,
) -> Result<(), axum::Error> {
    let json = serde_json::to_string(frame).expect("ServerFrame is always serializable");
    socket.send(Message::Text(json.into())).await
}
