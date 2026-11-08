use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;

use tokio::net::UdpSocket;
use tokio::sync::mpsc;

use super::config::PlaneConfig;
use super::demux_setup::{create_session, route_datagram};
use super::inbox::{PlaneCommand, SessionInbound, SessionNotice};
use super::registries::{Registries, unbind_player};
use super::types::UplinkEvent;

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

                    None => {}
                }
            }
        }
    }

    for (_sid, tx) in sessions.iter() {
        let _ = tx.send(SessionInbound::Close);
    }
}
