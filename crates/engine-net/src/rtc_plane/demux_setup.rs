use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;

use tokio::net::UdpSocket;
use tokio::sync::mpsc;

use super::inbox::{SessionInbound, SessionNotice};
use super::registries::Registries;
use super::session::RtcSession;
use super::task::session_task;
use super::types::{RtcPlaneError, UplinkEvent};

pub(super) fn route_datagram(
    from: SocketAddr,
    bytes: Arc<[u8]>,
    addr_routes: &HashMap<SocketAddr, Arc<str>>,
    unbound: &HashSet<Arc<str>>,
    sessions: &HashMap<Arc<str>, mpsc::Sender<SessionInbound>>,
) {
    if let Some(sid) = addr_routes.get(&from) {
        if let Some(tx) = sessions.get(sid) {
            let _ = tx.try_send(SessionInbound::Datagram { from, bytes });
        }
        return;
    }
    for sid in unbound {
        if let Some(tx) = sessions.get(sid) {
            let _ = tx.try_send(SessionInbound::Datagram {
                from,
                bytes: bytes.clone(),
            });
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
    uplink_tx: &mpsc::UnboundedSender<UplinkEvent>,
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
