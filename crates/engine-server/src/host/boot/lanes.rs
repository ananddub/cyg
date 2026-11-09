use std::net::SocketAddr;

use engine_config::{GameConfig, Lane};
use engine_net::QuicTransport;
use engine_net::transport::WsTransport;
pub(crate) struct Lanes {
    pub(crate) quic: Option<QuicTransport>,
    pub(crate) ws: Option<WsTransport>,
}

/// Bind the native UDP lane when the config asks for it.
///
/// The port is ephemeral: it was previously a hardcoded `7778` that collided
/// with any other UDP lane and was invisible in the config. The OS assigns it and
/// the chosen port is logged, so a native-plugin client has something to dial.
pub(crate) fn boot_udp(enabled: bool) -> Option<std::net::UdpSocket> {
    if !enabled {
        tracing::info!("[server] udp lane off ([transports] udp = false)");
        return None;
    }
    let udp_socket = std::net::UdpSocket::bind("0.0.0.0:0")
        .or_else(|_| std::net::UdpSocket::bind("127.0.0.1:0"))
        .ok();
    match udp_socket {
        Some(ref s) => {
            let _ = s.set_nonblocking(true);
            match s.local_addr() {
                Ok(addr) => {
                    tracing::info!("[server] listening on udp://{addr} (native engine plugins)")
                }
                Err(_) => tracing::info!("[server] listening on an ephemeral udp port"),
            }
            udp_socket
        }
        None => {
            tracing::error!(
                "[server] ⚠️ could not bind the udp lane — native UDP clients disabled"
            );
            None
        }
    }
}

pub(crate) fn boot_lanes(cfg: &GameConfig, schema_descriptor: &[u8]) -> Lanes {
    let quic_transport = if cfg.transports.enabled(Lane::Quic) {
        // Ephemeral: a fixed port here collided with the native UDP lane and was
        // never something the operator chose.
        quic_listen(cfg.port)
    } else {
        tracing::info!("[server] quic lane off ([transports] quic = false)");
        None
    };
    let ws_transport = if cfg.transports.enabled(Lane::WebSocket) {
        ws_listen(cfg, schema_descriptor)
    } else {
        tracing::info!("[server] websocket lane off ([transports] websocket = false)");
        None
    };
    Lanes {
        quic: quic_transport,
        ws: ws_transport,
    }
}

/// QUIC on a free UDP port.
///
/// The preferred port is deliberately *next to* the game port rather than a
/// fixed constant, and a failed bind falls back to an ephemeral one instead of
/// disabling the lane: a busy neighbour port must not silently cost the operator
/// a transport they explicitly enabled.
fn quic_listen(game_port: u16) -> Option<QuicTransport> {
    let preferred = game_port.saturating_add(3);
    for candidate in [preferred, 0] {
        let addr = SocketAddr::new(
            std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED),
            candidate,
        );
        match engine_net::QuicTransport::new(addr) {
            Ok(q) => {
                tracing::info!(
                    "[server] 🚄 QUIC/WebTransport listening on udp://{addr} ({})",
                    Lane::Quic.purpose()
                );
                return Some(q);
            }
            Err(e) if candidate == preferred => {
                tracing::warn!(
                    "[server] quic: port {preferred} unavailable ({e}), taking an ephemeral one"
                );
            }
            Err(e) => {
                tracing::error!("[server] ⚠️ QUIC lane could not bind: {e}");
            }
        }
    }
    None
}

/// WebSocket room-data lane.
fn ws_listen(cfg: &GameConfig, schema_descriptor: &[u8]) -> Option<WsTransport> {
    for candidate in [cfg.port, 0] {
        match engine_net::transport::WsTransport::new(std::net::SocketAddr::from((
            [0, 0, 0, 0],
            candidate,
        ))) {
            Ok(mut ws) => {
                ws.set_schema_descriptor(schema_descriptor.to_vec());
                ws.set_udp_port(cfg.port);
                ws.set_room_variants(cfg.rooms.clone());
                tracing::info!(
                    "[server] 🕹️  WebSocket room-data lane on ws://0.0.0.0:{candidate} ({})",
                    Lane::WebSocket.purpose()
                );
                return Some(ws);
            }
            Err(e) if candidate == cfg.port => {
                tracing::warn!(
                    "[server] websocket: port {} unavailable ({e}), taking an ephemeral one",
                    cfg.port
                );
            }
            Err(e) => {
                tracing::error!("[server] ⚠️ WebSocket lane could not bind: {e}");
            }
        }
    }
    None
}
