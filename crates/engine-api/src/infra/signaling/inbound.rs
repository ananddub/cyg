use crate::services::plane::Plane;

use axum::extract::ws::{Message, WebSocket};

use crate::infra::seats::Seat;

use super::SignalIn;
use super::wire::{ClientFrame, ServerFrame};

/// Caps on client-supplied signaling blobs, enforced before anything is
/// posted to the hub.
const MAX_SDP_LEN: usize = 16 * 1024;
const MAX_CANDIDATE_LEN: usize = 2 * 1024;

pub(super) async fn handle_text(
    socket: &mut WebSocket,
    plane: &Plane,
    seat: &Seat,
    text: &str,
) -> bool {
    match serde_json::from_str::<ClientFrame>(text) {
        Ok(ClientFrame::Offer { sdp }) => {
            if sdp.len() > MAX_SDP_LEN {
                oversize(socket, "offer").await;
                return true;
            }
            if !plane.signals.post(SignalIn::Offer {
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
            if candidate.len() > MAX_CANDIDATE_LEN {
                oversize(socket, "candidate").await;
                return true;
            }
            if !plane.signals.post(SignalIn::Candidate {
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
            tracing::debug!(
                "[signaling] malformed frame from seat {} ({} bytes): {e}",
                seat.id,
                text.len()
            );
            let _ = send_frame(
                socket,
                &ServerFrame::Error {
                    message: "malformed signaling frame".into(),
                },
            )
            .await;
            true
        }
    }
}

async fn oversize(socket: &mut WebSocket, what: &str) {
    let _ = send_frame(
        socket,
        &ServerFrame::Error {
            message: format!("{what} exceeds the allowed size"),
        },
    )
    .await;
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
