//! The demultiplexer task: sole reader of the UDP socket and owner of the
//! address registry.

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;

use tokio::net::UdpSocket;
use tokio::sync::mpsc;

use super::config::PlaneConfig;
use super::inbox::{PlaneCommand, SessionInbound, SessionNotice};
use super::registries::{unbind_player, Registries};
use super::session::RtcSession;
use super::task::session_task;
use super::types::{RtcPlaneError, UplinkEvent};

/// Runs the demultiplexer to shutdown. Owned by the plane thread; its maps need no
/// lock because it is the sole reader of the socket, so two sessions never
/// serialize against each other here — only the sender registries are shared, and
/// those are locked briefly.
pub(crate) async fn demux_loop(
    socket: Arc<UdpSocket>,
    local_dest: SocketAddr,
    mut cmd_rx: mpsc::UnboundedReceiver<PlaneCommand>,
    mut notice_rx: mpsc::UnboundedReceiver<SessionNotice>,
    notice_tx: mpsc::UnboundedSender<SessionNotice>,
    uplink_tx: mpsc::UnboundedSender<UplinkEvent>,
    registries: Registries,
    config: PlaneConfig,
) {
    let mut sessions: HashMap<Arc<str>, mpsc::Sender<SessionInbound>> = HashMap::new();
    let mut addr_routes: HashMap<SocketAddr, Arc<str>> = HashMap::new();
    // Sessions without a pinned remote address get a copy of every unknown datagram.
    let mut unbound: HashSet<Arc<str>> = HashSet::new();

    let mut buf = vec![0u8; 65_535];

    loop {
        tokio::select! {
            recv = socket.recv_from(&mut buf) => {
                if let Ok((n, from)) = recv {
                    if n > 0 {
                        let bytes: Arc<[u8]> = Arc::from(&buf[..n]);
                        route_datagram(from, bytes, &addr_routes, &unbound, &sessions);
                    }
                }
            }
            cmd = cmd_rx.recv() => {
                let Some(cmd) = cmd else { break };
                match cmd {
                    PlaneCommand::Shutdown => break,
                    PlaneCommand::CreateSession { session_id, player_id, reply } => {
                        let outcome = create_session(
                            session_id,
                            player_id,
                            local_dest,
                            &socket,
                            &mut sessions,
                            &mut unbound,
                            &registries,
                            &notice_tx,
                            &uplink_tx,
                            config.session_inbox_capacity,
                        );
                        let _ = reply.send(outcome);
                    }
                    PlaneCommand::Forward { session_id, msg } => {
                        let closing = matches!(msg, SessionInbound::Close);
                        if let Some(tx) = sessions.get(&session_id) {
                            // A full inbox means the session is wedged: drop the message rather than stall the sole socket reader.
                            let _ = tx.try_send(msg);
                        } else if closing {
                            unbind_player(&session_id, &registries);
                        }
                    }
                }
            }
            notice = notice_rx.recv() => {
                match notice {
                    Some(SessionNotice::RemoteAddrBound { session_id, addr }) => {
                        unbound.remove(&session_id);
                        addr_routes.insert(addr, session_id);
                    }
                    Some(SessionNotice::Gone { session_id }) => {
                        sessions.remove(&session_id);
                        unbound.remove(&session_id);
                        addr_routes.retain(|_, sid| sid != &session_id);
                        unbind_player(&session_id, &registries);
                    }
                    // notice_rx closes only once every session task dropped; keep serving.
                    None => {}
                }
            }
        }
    }

    // On shutdown, ask every live session to stop.
    for (_sid, tx) in sessions.iter() {
        let _ = tx.send(SessionInbound::Close);
    }
}

/// Route one received datagram. Fast path is a known remote address; otherwise
/// copy the (Arc-backed) datagram to every session still searching for its peer.
fn route_datagram(
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
            let _ = tx.try_send(SessionInbound::Datagram { from, bytes: bytes.clone() });
        }
    }
}

/// Build a session and spawn its task. Runs synchronously inside the demux task
/// (no await), so it mutates the demux-owned maps directly.
#[allow(clippy::too_many_arguments)]
fn create_session(
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
