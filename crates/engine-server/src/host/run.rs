use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use engine_bus::Bus;
use engine_core::MessageRateLimits;

use crate::telemetry::Recorder;

use crate::host::*;
use engine_config::{BootMaps, BootModule, GameConfig};

pub fn run(
    project_dir: &Path,
    wasm_path: &Path,
    cfg: &GameConfig,
    bus: Arc<Bus>,
    signals: Option<engine_api::infra::signaling::SignalHub>,
    boot_maps: BootMaps,
    boot_module: Option<BootModule>,
) -> std::io::Result<()> {
    // Refuse a config with no lane before opening anything, and say which lanes
    // this process is about to serve so the log answers "what is listening?"
    // without needing to correlate four separate boot lines.
    if let Err(problem) = cfg.transports.validate() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            problem,
        ));
    }
    tracing::info!("[server] {}", cfg.transports.summary());
    let (mut runtime, _epoch_ticker, mut raw_transport) = boot_runtime(wasm_path, cfg);
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
    let arena_mesh = boot_oxcl(&boot_maps, project_dir);
    let arena_2d = boot_map2d(&boot_maps, project_dir);
    let physics = boot_map3d(&boot_maps, project_dir, &arena_2d);
    let (schema_descriptor, schema_aoi) =
        boot_schema(&boot_maps, project_dir, cfg, &mut raw_transport);
    let join_options_fields = if schema_descriptor.is_empty() {
        Vec::new()
    } else {
        engine_schema::parse_descriptor(&schema_descriptor)
            .and_then(|descriptor| {
                descriptor
                    .schemas
                    .into_iter()
                    .find(|schema| schema.name == engine_schema::JOIN_OPTIONS_SCHEMA_NAME)
            })
            .map(|schema| schema.fields)
            .unwrap_or_default()
    };
    let udp_socket = boot_udp(cfg.transports.enabled(engine_config::Lane::Udp));
    let Lanes { quic, ws } = boot_lanes(cfg, &schema_descriptor);
    let mut transport = LoopTransport::new(raw_transport, quic, ws);
    if cfg.simulate_latency_ms > 0 {
        transport.set_latency(Duration::from_millis(cfg.simulate_latency_ms as u64 / 2));
        tracing::warn!(
            "[server] NETWORK SIMULATION: simulated latency = {}ms RTT ({}ms one-way)",
            cfg.simulate_latency_ms,
            cfg.simulate_latency_ms / 2
        );
    }
    if cfg.simulate_packet_loss > 0.0 {
        transport.set_packet_loss_rate(cfg.simulate_packet_loss);
        tracing::warn!(
            "[server] ⚠️ NETWORK SIMULATION: simulated packet loss = {:.1}%",
            cfg.simulate_packet_loss * 100.0
        );
    }
    let PreSelection {
        pre,
        console_active,
        wasm_available,
    } = select_pre_module(&runtime, boot_module.as_ref(), wasm_path, cfg);
    let mut server = assemble_server(pre, cfg, physics, arena_2d, &boot_maps);
    rehydrate_persistent_rooms(&mut server, &bus, cfg, REHYDRATE_WAIT);
    let signal_rx = signals.as_ref().and_then(|hub| hub.take_signals());
    let game_id = cfg
        .game_id
        .clone()
        .or(cfg.game_key.clone())
        .unwrap_or_else(|| cfg.name.clone());
    let node_id = engine_config::node_id();
    let source_path = project_dir.join(&cfg.source);
    let mut engine = Engine {
        cfg,
        bus,
        game_cfg: Arc::new(cfg.gameplay.clone()),
        _epoch_ticker,
        hot: HotReload {
            runtime: &mut runtime,
            project_dir,
            wasm_path,
            source_path: source_path.clone(),
            console_active,
            wasm_available,
            last_wasm_check: Instant::now(),
            loaded_wasm_time: if wasm_available {
                std::fs::metadata(wasm_path)
                    .ok()
                    .and_then(|m| m.modified().ok())
            } else {
                None
            },
            loaded_src_time: std::fs::metadata(&source_path)
                .ok()
                .and_then(|m| m.modified().ok()),
        },
        signals: &signals,
        game_id,
        node_id,
        transport,
        server,
        arena_mesh,
        join_options_fields,
        udp: UdpLane {
            udp_sock: udp_socket.map(Arc::new),
            udp_peers: HashMap::new(),
            udp_reliable: HashMap::new(),
            udp_rxd_seq: HashMap::new(),
            udp_input_rates: HashMap::new(),
            udp_join_rates: HashMap::new(),
            rudp_scratch: Vec::with_capacity(2100),
            schema_descriptor,
            timed_out_udp: Vec::new(),
        },
        signal_rx,
        drain: Drain::new(Duration::from_secs(cfg.lifecycle.drain_grace_secs)),
        scratch: Scratch {
            events: Vec::with_capacity(1024),
        },
        rates: Rates {
            rtc_input_rates: HashMap::new(),
            message_rate_limits: MessageRateLimits::new(cfg.max_messages_per_second),
            last_input_at: HashMap::new(),
        },
        clock: Clock {
            tick_interval: Duration::from_secs_f64(1.0 / cfg.tick_rate.max(1) as f64),
        },
        stats: Stats {
            recorder: Recorder::new(),
            publish_start: Instant::now(),
        },
        tick: TickRunner {
            session_scratch: Vec::with_capacity(64),
            room_names: Vec::new(),
            delta_buf: Vec::with_capacity(8 * 1024),
            view_buf: Vec::with_capacity(8 * 1024),
            delta_body: Vec::with_capacity(8 * 1024),
            filtered_buf: Vec::with_capacity(8 * 1024),
            udp_batch: Vec::new(),
            schema_aoi,
            dead_rooms: Vec::new(),
            last_tick: Instant::now(),
            tick_counter: 0,
            tick_stat_start: Instant::now(),
            window_ticks: 0,
            stage_sim_us_sum: 0,
            stage_guest_us_sum: 0,
            match_peaks: HashMap::new(),
            next_phase: 0,
            slot_base: Instant::now(),
            interval_pending: 0,
        },
    };
    // Profiling builds exit cleanly after CYG_DHAT_SECS / CYG_CPU_PROF_SECS so
    // the profiler drops and writes its report (release runs never exit on
    // their own). Runtime env check (not cfg): crate features don't propagate
    // from the bin.
    let dhat_deadline = std::env::var("CYG_DHAT_SECS")
        .ok()
        .or_else(|| std::env::var("CYG_CPU_PROF_SECS").ok())
        .and_then(|v| v.parse::<u64>().ok())
        .map(|secs| std::time::Instant::now() + std::time::Duration::from_secs(secs));
    // Return freed arena pages to the OS on quiet ticks — glibc holds freed
    // memory for reuse, which inflated post-disconnect RSS to ~2.1GB in
    // loadtests even with zero clients. Trim only when the room set is
    // idle-ish; trimming under load would fight the allocator.
    let mut last_trim = std::time::Instant::now();
    loop {
        if last_trim.elapsed().as_secs() >= 60 && engine.room_count() <= 1 {
            unsafe { libc::malloc_trim(0) };
            last_trim = std::time::Instant::now();
        }
        if engine.step()? {
            tracing::info!("[server] 🛑 drain complete — shutting down");
            break;
        }
        if let Some(deadline) = dhat_deadline {
            if std::time::Instant::now() >= deadline {
                tracing::info!("[dhat] window elapsed — clean exit for heap dump");
                break;
            }
        }
    }
    Ok(())
}
