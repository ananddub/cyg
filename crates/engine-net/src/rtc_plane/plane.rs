use std::net::SocketAddr;
use std::sync::Arc;
use std::thread::JoinHandle;

use tokio::net::UdpSocket;
use tokio::sync::mpsc;

use super::config::PlaneConfig;
use super::demux::demux_loop;
use super::handle::PlaneHandle;
use super::types::UplinkEvent;
use super::inbox::PlaneCommand;
use super::registries::Registries;
use super::stats::DemuxStats;

pub struct Plane {
    thread: Option<JoinHandle<()>>,
    cmd_tx: mpsc::UnboundedSender<PlaneCommand>,
}

impl Plane {
    pub fn spawn(
        bind_addr: SocketAddr,
        announced_addr: SocketAddr,
        config: PlaneConfig,
    ) -> std::io::Result<(Plane, PlaneHandle)> {
        let std_socket = std::net::UdpSocket::bind(bind_addr)?;
        std_socket.set_nonblocking(true)?;
        let local_addr = std_socket.local_addr()?;
        let local_dest = if announced_addr.ip().is_unspecified() {
            SocketAddr::new(
                std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
                local_addr.port(),
            )
        } else {
            announced_addr
        };

        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        // Bounded uplink: session tasks get natural backpressure via
        // `send().await` instead of queueing an unbounded memory spike under
        // an input burst (books: bound your channels — a full 4096-event
        // backlog is a stalled consumer, not a loss).
        let (uplink_tx, uplink_rx) = mpsc::channel::<UplinkEvent>(4096);
        let (notice_tx, notice_rx) = mpsc::unbounded_channel();
        let registries = Registries::new();
        let stats = DemuxStats::new();

        let handle = PlaneHandle {
            cmd_tx: cmd_tx.clone(),
            registries: registries.clone(),
            uplink: Arc::new(tokio::sync::Mutex::new(uplink_rx)),
            stats: stats.clone(),
        };

        let thread = std::thread::Builder::new()
            .name("rtc-plane".to_string())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_time()
                    .enable_io()
                    .build()
                    .expect("tokio current-thread runtime");
                rt.block_on(async move {
                    let socket = Arc::new(UdpSocket::from_std(std_socket).expect("udp from_std"));
                    demux_loop(
                        socket, local_dest, cmd_rx, notice_rx, notice_tx, uplink_tx, registries,
                        config, stats,
                    )
                    .await;
                });
            })?;

        Ok((
            Plane {
                thread: Some(thread),
                cmd_tx,
            },
            handle,
        ))
    }

    pub fn shutdown(mut self) {
        let _ = self.cmd_tx.send(PlaneCommand::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Plane {
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            let _ = self.cmd_tx.send(PlaneCommand::Shutdown);
            let _ = thread.join();
        }
    }
}
