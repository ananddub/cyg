use std::net::SocketAddr;
use std::path::Path;

use engine_net::transport::WebRtcTransport;
use engine_wasm::WasmRuntime;

use crate::host::*;
use engine_config::GameConfig;
pub(crate) fn boot_runtime(
    wasm_path: &Path,
    cfg: &GameConfig,
) -> (WasmRuntime, Option<EpochTicker>, WebRtcTransport) {
    let runtime = WasmRuntime::new_with_config(engine_wasm::WasmConfig {
        optimize_for_speed: true,
        enable_epoch_interruption: cfg.watchdog.enabled,
        epoch_deadline_ticks: cfg
            .watchdog
            .enabled
            .then_some(cfg.watchdog.epoch_deadline_ticks),
        use_pooling_allocator: cfg.wasm_pooling,
        ..engine_wasm::WasmConfig::default()
    })
    .expect("wasm runtime");

    let _epoch_ticker = cfg
        .watchdog
        .enabled
        .then(|| runtime.spawn_epoch_ticker(cfg.watchdog.epoch_period()));
    match std::fs::metadata(wasm_path) {
        Ok(m) => tracing::info!(
            "[server] room logic loaded ({} bytes) — generic byte pipeline active",
            m.len()
        ),
        Err(_) => tracing::info!(
            "[server] no room logic file at {} — Unity-relay rooms only",
            wasm_path.display()
        ),
    }

    let webrtc_port: u16 = cfg.port;
    let webrtc_udp = std::net::UdpSocket::bind(format!("0.0.0.0:{webrtc_port}"))
        .or_else(|_| std::net::UdpSocket::bind("0.0.0.0:0"))
        .ok();
    let raw_transport = if let Some(sock) = webrtc_udp {
        let actual_port = sock.local_addr().map(|a| a.port()).unwrap_or(webrtc_port);
        tracing::info!(
            "[server] ⚡ WebRTC DataChannel server listening on udp://0.0.0.0:{actual_port}"
        );
        let local_addr: SocketAddr = format!("127.0.0.1:{actual_port}").parse().unwrap();
        let mut announced = vec![local_addr];
        let lan_ip = std::net::UdpSocket::bind("0.0.0.0:0")
            .and_then(|s| s.connect("8.8.8.8:80").map(|_| s.local_addr()))
            .ok()
            .and_then(|a| a.ok())
            .map(|a| a.ip());
        if let Some(ip) = lan_ip {
            let lan_addr = SocketAddr::new(ip, actual_port);
            if lan_addr != local_addr {
                announced.push(lan_addr);
                tracing::info!("[server] ⚡ WebRTC announced LAN candidate: {lan_addr}");
            }
        }
        if let Ok(ip) = std::env::var("PUBLIC_IP")
            && let Ok(a) = format!("{ip}:{actual_port}").parse()
        {
            announced.push(a);
        }
        WebRtcTransport::from_socket(sock, announced).expect("webrtc transport")
    } else {
        tracing::error!(
            "[server] ⚠️ webrtc bind :{webrtc_port} failed — WebRTC data plane disabled"
        );
        WebRtcTransport::from_socket(
            std::net::UdpSocket::bind("127.0.0.1:0").expect("ephemeral udp bind"),
            Vec::new(),
        )
        .expect("ephemeral webrtc transport")
    };

    (runtime, _epoch_ticker, raw_transport)
}
