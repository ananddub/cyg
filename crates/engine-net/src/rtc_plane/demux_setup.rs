use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;

use bytes::Bytes;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;

use super::inbox::{SessionInbound, SessionNotice};
use super::registries::Registries;
use super::session::RtcSession;
use super::stats::DemuxStats;
use super::task::session_task;
use super::types::{RtcPlaneError, UplinkEvent};

/// Hands a datagram to its session inbox. The inbox is bounded, so the send
/// is a `try_send`: a session that cannot keep up loses this packet rather
/// than stalling the demux loop, and the loss is counted on the plane stats.
pub(super) fn route_datagram(
    from: SocketAddr,
    bytes: Bytes,
    addr_routes: &HashMap<SocketAddr, Arc<str>>,
    unbound: &HashSet<Arc<str>>,
    sessions: &HashMap<Arc<str>, mpsc::Sender<SessionInbound>>,
    stats: &DemuxStats,
) {
    if let Some(sid) = addr_routes.get(&from) {
        deliver(
            sid,
            sessions,
            SessionInbound::Datagram { from, bytes },
            stats,
        );
        return;
    }
    for sid in unbound {
        deliver(
            sid,
            sessions,
            SessionInbound::Datagram {
                from,
                bytes: bytes.clone(),
            },
            stats,
        );
    }
}

pub(super) fn deliver(
    session_id: &Arc<str>,
    sessions: &HashMap<Arc<str>, mpsc::Sender<SessionInbound>>,
    msg: SessionInbound,
    stats: &DemuxStats,
) {
    if let Some(tx) = sessions.get(session_id) {
        match tx.try_send(msg) {
            Ok(()) => {}
            // A closed inbox means the session task is gone; the `Gone`
            // notice cleans that up, so it is not counted as backpressure.
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                stats.record_inbound_drop(session_id);
            }
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn create_session(
    session_id: Arc<str>,
    player_id: Arc<str>,
    local_dest: SocketAddr,
    socket: &Arc<UdpSocket>,
    sessions: &mut HashMap<Arc<str>, mpsc::Sender<SessionInbound>>,
    unbound: &mut HashSet<Arc<str>>,
    registries: &Registries,
    notice_tx: &mpsc::UnboundedSender<SessionNotice>,
    uplink_tx: &mpsc::Sender<UplinkEvent>,
    inbox_capacity: usize,
) -> Result<String, RtcPlaneError> {
    let (rtc, offer_sdp) = RtcSession::new_server(session_id.clone(), local_dest, &[local_dest])?;

    let (tx, rx) = mpsc::channel::<SessionInbound>(inbox_capacity.max(1));
    sessions.insert(session_id.clone(), tx.clone());
    unbound.insert(session_id.clone());
    registries.insert_session(session_id.clone(), tx);
    registries.bind_player(player_id.clone(), session_id.clone());

    tokio::spawn(session_task(
        rtc,
        player_id,
        rx,
        Arc::clone(socket),
        uplink_tx.clone(),
        notice_tx.clone(),
    ));

    Ok(offer_sdp)
}
