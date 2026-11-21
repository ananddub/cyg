use std::io;
use std::net::ToSocketAddrs;
use std::path::Path;
use std::sync::Arc;

use engine_api::{ApiState, gc_seats, pump, pump_events, serve, sweep_match_history};
use engine_bus::Bus;
use engine_config::{ControlConfig, GameConfig};

use crate::plane::{ControlPlane, shutdown_signal};
use crate::secrets::{describe_boot_maps, refuse_default_secrets};

/// Accept-queue depth for the control plane. The kernel clamps this to its own
/// ceiling (`net.core.somaxconn` on Linux, `SOMAXCONN` elsewhere), so asking for
/// more than the platform allows is harmless and only removes our ceiling.
const CONTROL_LISTEN_BACKLOG: i32 = 8192;

/// Bind a TCP listener with an explicit accept-queue depth.
///
/// `tokio::net::TcpListener::bind` hardcodes a 1024-deep queue. A join storm
/// (thousands of clients at once) overflows it and the kernel drops the excess
/// SYNs, so clients see a failed upgrade.
///
/// Portable on purpose: the address family is derived from the configured
/// bind address rather than assumed to be IPv4, and `set_reuse_port` is only
/// requested where the platform implements the socket option (Linux/BSD).
fn bind_with_backlog(addr: std::net::IpAddr, port: u16) -> io::Result<std::net::TcpListener> {
    let domain = match addr {
        std::net::IpAddr::V4(_) => socket2::Domain::IPV4,
        std::net::IpAddr::V6(_) => socket2::Domain::IPV6,
    };
    let sock = socket2::Socket::new(domain, socket2::Type::STREAM, Some(socket2::Protocol::TCP))?;
    sock.set_reuse_address(true)?;
    #[cfg(all(
        unix,
        not(target_os = "solaris"),
        not(target_os = "illumos"),
        not(target_os = "emscripten")
    ))]
    {
        // Never fatal: SO_REUSEPORT is an optimisation for several listeners
        // on one port, not a requirement for a single control plane.
        let _ = sock.set_reuse_port(true);
    }
    sock.set_nonblocking(true)?;
    sock.bind(&std::net::SocketAddr::new(addr, port).into())?;
    sock.listen(CONTROL_LISTEN_BACKLOG)?;
    Ok(sock.into())
}

pub fn spawn(
    project_dir: &Path,
    game: &GameConfig,
    control: &ControlConfig,
    bus: Arc<Bus>,
    manager: Arc<engine_manager::ServerManager>,
) -> std::io::Result<ControlPlane> {
    refuse_default_secrets(game, control)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(control.workers.max(1))
        .thread_name("cyg-control")
        // SurrealDB 4's bytecode query executor recurses deeper than 3.x —
        // the 2 MiB default overflowed during the boot migrations on a
        // fresh store. 16 MiB keeps the executor and nested serde decoding
        // comfortably inside the guard page.
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()?;

    // `bind_addr` is normally a bare IP (`0.0.0.0`), but operators write
    // hostnames too. Resolve it so a name like "localhost" keeps working and
    // the address family still comes from whatever it resolves to.
    let bind_addr: std::net::IpAddr = if let Ok(ip) = control.bind_addr.parse() {
        ip
    } else {
        std::net::ToSocketAddrs::to_socket_addrs(&(control.bind_addr.as_str(), control.api_port))?
            .next()
            .map(|sa| sa.ip())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("control.bind_addr '{}' did not resolve", control.bind_addr),
                )
            })?
    };
    let listener = runtime.block_on(async {
        let std_listener = bind_with_backlog(bind_addr, control.api_port)?;
        tokio::net::TcpListener::from_std(std_listener)
    })?;
    let port = listener.local_addr()?.port();

    let state = runtime.block_on(ApiState::new(project_dir, game, control, bus, port));
    // Let the HTTP surface fail joins fast while the host is booting or
    // draining, using the same authority the host loop drives.
    engine_api::install_lifecycle(manager.clone());

    runtime.spawn(serve(listener, state.clone(), shutdown_signal()));

    let boot_maps = runtime.block_on(engine_api::services::maps::MapService::load_boot_maps(
        &state,
    ));
    if !boot_maps.is_empty() {
        tracing::info!(
            "[control] 🗺️  active maps from console: {}",
            describe_boot_maps(&boot_maps)
        );
    }

    let boot_module = runtime.block_on(engine_api::services::wasm::WasmService::load_boot_module(
        &state,
    ));
    if let Some(module) = boot_module.as_ref() {
        tracing::info!(
            "[control] 📦 active room module from console: v{} ({} bytes, sha256 {}…)",
            module.version,
            module.bytes.len(),
            &module.sha256[..module.sha256.len().min(12)]
        );
    }

    runtime.spawn(pump(state.clone()));

    runtime.spawn(gc_seats(state.clone()));

    runtime.spawn(pump_events(state.clone()));
    runtime.spawn(sweep_match_history(state.clone()));

    // The control plane is a managed process thread group: register it with the
    // lifecycle so the same authority that owns the host loop also owns this.
    manager.register_thread(engine_manager::ThreadRole::Control, "cyg-control");
    crate::agones::spawn(&state, &manager);

    {
        let state = state.clone();
        let agones = control.agones.enabled;
        runtime.spawn(async move {
            shutdown_signal().await;
            state.bus.request_drain();
            if agones {
                match crate::agones::shutdown() {
                    Ok(sent) => tracing::info!(
                        "[agones] 🛑 {} shutdown sent; this pod leaves the pool",
                        if sent { "first" } else { "repeat" }
                    ),
                    Err(e) => tracing::error!("[agones] ✖ could not retire the pod: {e}"),
                }
            }
        });
    }

    Ok(ControlPlane {
        runtime,
        state,
        port,
        boot_maps,
        boot_module,
        manager,
    })
}
