use std::net::SocketAddr;

use engine_config::GameConfig;
use engine_net::QuicTransport;
use engine_net::transport::WsTransport;
pub(crate) struct Lanes {
    pub(crate) quic: Option<QuicTransport>,
    pub(crate) ws: Option<WsTransport>,
}

pub(crate) fn boot_udp() -> Option<std::net::UdpSocket> {
    let udp_addr = std::env::var("OX_UDP_ADDR").unwrap_or_else(|_| "0.0.0.0:7778".into());
    let udp_socket = std::net::UdpSocket::bind(&udp_addr).ok();
    if let Some(ref s) = udp_socket {
        let _ = s.set_nonblocking(true);
        match s.local_addr() {
            Ok(addr) => {
                tracing::info!(
                    "[server] listening on udp://{addr} (Unity / Unreal / Godot Native Engine)"
                );
            }
            Err(_) => tracing::info!(
                "[server] listening on udp://{udp_addr} (Unity / Unreal / Godot Native Engine)"
            ),
        }
    } else {
        tracing::error!("[server] ⚠️ udp bind {udp_addr} failed — native UDP clients disabled");
    }
    udp_socket
}

pub(crate) fn boot_lanes(cfg: &GameConfig, schema_descriptor: &[u8]) -> Lanes {
    let quic_transport = if cfg.quic_port > 0 {
        let addr: SocketAddr = SocketAddr::new(
            std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED),
            cfg.quic_port,
        );
        match engine_net::QuicTransport::new(addr) {
            Ok(q) => {
                tracing::info!(
                    "[server] 🚄 QUIC/WebTransport listening on udp://{addr} (same wire contract as WebRTC)"
                );
                Some(q)
            }
            Err(e) => {
                tracing::error!(
                    "[server] ⚠️ QUIC bind :{} failed — lane disabled ({e})",
                    cfg.quic_port
                );
                None
            }
        }
    } else {
        None
    };
    let ws_transport = if cfg.ws_port > 0 {
        match engine_net::transport::WsTransport::new(std::net::SocketAddr::from((
            [0, 0, 0, 0],
            cfg.ws_port,
        ))) {
            Ok(mut ws) => {
                ws.set_schema_descriptor(schema_descriptor.to_vec());
                ws.set_udp_port(cfg.port);
                ws.set_room_variants(cfg.rooms.clone());
                tracing::info!(
                    "[server] 🕹️  WebSocket room-data lane on ws://0.0.0.0:{} (Godot / Unity / Unreal / cyg.js)",
                    cfg.ws_port
                );
                Some(ws)
            }
            Err(e) => {
                tracing::error!(
                    "[server] ⚠️ ws bind 0.0.0.0:{} failed — WebSocket clients disabled: {e}",
                    cfg.ws_port
                );
                None
            }
        }
    } else {
        None
    };
    Lanes {
        quic: quic_transport,
        ws: ws_transport,
    }
}
