//! cyg-loadtest — Rust replacement for `bench/loadtest.mjs`.
//!
//! Spawns N schema clients against a running game server, drives inputs at a
//! fixed rate, and reports join latency, time-to-first-snapshot, update rate
//! and counted traffic. Unlike the JS tool it does NOT reimplement the
//! protocol: every descriptor parse, OXS2 decode and input encode goes through
//! `cyg-core-rs` — the same core the web/Unity/Godot SDKs ship.
//!
//!   cyg-loadtest --url http://127.0.0.1:8297 --room arena \
//!       --clients 16 --seconds 10 --hz 30 --move --json
//!
//! Per client (one std thread): HTTP matchmake → WebSocket room-data lane →
//! "OXSD" descriptor frame builds the core → every delta frame goes through
//! `ClientCore::apply_payload` → inputs go out via
//! `ClientCore::encode_schema_input` at --hz.

use std::collections::BTreeMap;
use std::net::TcpStream;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use clap::Parser;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::WebSocket;

use cyg_core_rs::input::InputValue;
use cyg_core_rs::ClientCore;
use cyg_loadtest::{
    build_ws_url, classify_frame, delta_kind, encode_schema_input_with, encode_schema_ready_ack,
    is_full_kind, is_patch_kind, mean, movement_values, parse_seat, percentile, Inbound, Seat,
};

/// Per-client counters. `join_ms`/`ready_ms`/`errors` are live from the join
/// phase; the window fields are only filled in the final report.
#[derive(Default, Clone)]
struct ClientMetrics {
    index: usize,
    session_id: String,
    join_ms: f64,
    ready_ms: f64,
    ready: bool,
    fatal: bool,
    fulls_window: u64,
    patches_window: u64,
    bytes_in_window: u64,
    bytes_out_window: u64,
    inputs_window: u64,
    entities_min: usize,
    entities_max: usize,
    errors: Vec<String>,
}

struct SharedGate {
    window_start: Mutex<Option<Instant>>,
    cv: Condvar,
}

impl SharedGate {
    fn wait_for_window(&self) -> Instant {
        let mut guard = self.window_start.lock().unwrap();
        while guard.is_none() {
            guard = self.cv.wait(guard).unwrap();
        }
        guard.unwrap()
    }
}

fn main() {
    let args = Args::parse();
    std::process::exit(run(args));
}

#[derive(Parser, Debug, Clone)]
#[command(
    name = "cyg-loadtest",
    about = "CYG load test — Rust CLI driving the shared cyg-core-rs protocol core (replaces bench/loadtest.mjs)"
)]
struct Args {
    /// Matchmake control-plane endpoint
    #[arg(long, default_value = "http://127.0.0.1:8297")]
    url: String,
    /// Room name to join or create
    #[arg(long, default_value = "arena")]
    room: String,
    /// Join an existing room by id instead of --room
    #[arg(long)]
    room_id: Option<String>,
    /// Spread clients across "room-0" .. "room-(n-1)" instead of one room
    /// (matches the old JS tool's --rooms; rooms are created on demand)
    #[arg(long, default_value_t = 1)]
    rooms: usize,
    /// Number of clients
    #[arg(long, default_value_t = 16)]
    clients: usize,
    /// Measurement window after all clients are ready (seconds)
    #[arg(long, default_value_t = 10)]
    seconds: u64,
    /// Input messages per second per client
    #[arg(long, default_value_t = 30)]
    hz: u32,
    /// Send alternating left/right movement inputs on the schema input lane
    #[arg(long = "move")]
    move_inputs: bool,
    /// Max joins per second during the join phase (0 = all at once)
    #[arg(long, default_value_t = 0)]
    ramp: u32,
    /// Per-client time-to-first-snapshot timeout (ms)
    #[arg(long, default_value_t = 15000)]
    timeout_ms: u64,
    /// Print a machine-readable report
    #[arg(long)]
    json: bool,
}

fn run(args: Args) -> i32 {
    let gate = Arc::new(SharedGate {
        window_start: Mutex::new(None),
        cv: Condvar::new(),
    });
    let (tx, rx) = std::sync::mpsc::channel::<(bool, ClientMetrics)>();
    let mut handles = Vec::new();

    for index in 0..args.clients {
        let gate = gate.clone();
        let tx = tx.clone();
        let args = args.clone();
        let join_offset_ms = if args.ramp > 0 {
            (index as u64) * 1000 / args.ramp as u64
        } else {
            0
        };
        handles.push(std::thread::spawn(move || {
            // --ramp pacing: each thread sleeps its slot before touching
            // matchmake, so N threads never exceed ramp joins/second.
            if join_offset_ms > 0 {
                std::thread::sleep(Duration::from_millis(join_offset_ms));
            }
            client_main(index, &args, &gate, &tx);
        }));
    }
    drop(tx);

    // Join phase: collect each client's readiness signal; open the measurement
    // window once every client is ready (or bail out on fatal errors).
    let mut records: BTreeMap<usize, ClientMetrics> = BTreeMap::new();
    let join_deadline = Instant::now() + Duration::from_millis(args.timeout_ms + 5_000);
    let mut joined_so_far = 0usize;
    for _ in 0..args.clients {
        let remaining = join_deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(remaining) {
            Ok((is_final, m)) => {
                if !is_final {
                    if m.fatal {
                        eprintln!("client {} failed: {}", m.index, m.errors.join("; "));
                    } else if !args.json {
                        joined_so_far += 1;
                        eprintln!(
                            "joined {}/{} (session {}, snapshot {:.1} ms)",
                            joined_so_far,
                            args.clients,
                            m.session_id,
                            m.ready_ms
                        );
                    }
                    records.insert(m.index, m);
                }
            }
            Err(_) => break,
        }
    }

    let ready_count = records.values().filter(|m| m.ready && !m.fatal).count();
    let fatal_count = records.values().filter(|m| m.fatal).count();
    if ready_count == 0 {
        eprintln!("loadtest: no clients reached a full snapshot");
        return 2;
    }
    if ready_count < args.clients {
        let requested = args.clients;
        eprintln!("loadtest: only {ready_count}/{requested} clients ready ({fatal_count} fatal)");
        return 2;
    }

    // Open the measurement window: all client threads start counting here.
    let window_start = Instant::now();
    {
        let mut guard = gate.window_start.lock().unwrap();
        *guard = Some(window_start);
        gate.cv.notify_all();
    }
    std::thread::sleep(Duration::from_secs(args.seconds));
    let duration_sec = window_start.elapsed().as_secs_f64().max(0.001);

    // Collect the final per-client window reports.
    let final_deadline = Instant::now() + Duration::from_secs(5);
    for _ in 0..args.clients {
        let remaining = final_deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(remaining) {
            Ok((is_final, m)) if is_final => {
                records.insert(m.index, m);
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    let _ = &rx; // remaining threads may still send; dropping rx is fine

    let report = build_report(&args, &records, duration_sec, args.clients, ready_count);
    let errors_total = report["errors"].as_array().map(|a| a.len()).unwrap_or(0);
    if args.json {
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else {
        print_report(&report);
    }
    if errors_total == 0 && ready_count == args.clients {
        0
    } else {
        1
    }
}

fn client_main(
    index: usize,
    args: &Args,
    gate: &SharedGate,
    tx: &std::sync::mpsc::Sender<(bool, ClientMetrics)>,
) {
    let mut metrics = ClientMetrics {
        index,
        ..Default::default()
    };
    match join_phase(index, args, metrics.clone()) {
        Ok((ready_snapshot, phase)) => {
            metrics = ready_snapshot;
            let _ = tx.send((false, metrics.clone()));
            let window = measurement_phase(args, gate, phase);
            metrics.fulls_window = window.fulls;
            metrics.patches_window = window.patches;
            metrics.bytes_in_window = window.bytes_in;
            metrics.bytes_out_window = window.bytes_out;
            metrics.inputs_window = window.inputs;
            metrics.entities_min = window.entities_min;
            metrics.entities_max = window.entities_max;
            metrics.errors.extend(window.errors);
            let _ = tx.send((true, metrics));
        }
        Err((mut failed, error)) => {
            failed.fatal = true;
            failed.errors.push(error);
            let _ = tx.send((false, failed));
        }
    }
}

/// Everything a client needs after the join phase to run its measurement.
struct JoinedPhase {
    ws: WebSocket<MaybeTlsStream<TcpStream>>,
    core: ClientCore,
    input_fields: Vec<cyg_core_rs::SchemaField>,
    seq: u16,
    first_full_bytes: u64,
}

fn join_phase(index: usize, args: &Args, mut metrics: ClientMetrics) -> Result<(ClientMetrics, JoinedPhase), (ClientMetrics, String)> {
    let fail = |mut m: ClientMetrics, e: String| {
        m.errors.push(e.clone());
        Err((m, e))
    };

    // 1. Matchmake (joinMs covers matchmake + ws handshake, like the JS tool).
    let join_start = Instant::now();
    let target_room = if args.rooms > 1 {
        format!("{}-{}", args.room, index % args.rooms)
    } else {
        args.room.clone()
    };
    let (seat, _http_ms) = match matchmake(args, &target_room) {
        Ok(v) => v,
        Err(e) => return fail(metrics, e),
    };
    metrics.session_id = seat.session_id.clone();

    // 2. WebSocket to the room-data lane.
    let ws_url = build_ws_url(&args.url, &seat, client_room(index, args));
    let (mut ws, _) = match tungstenite::connect(ws_url.as_str()) {
        Ok(v) => v,
        Err(e) => return fail(metrics, format!("ws connect failed: {e}")),
    };
    metrics.join_ms = join_start.elapsed().as_secs_f64() * 1000.0;
    if let MaybeTlsStream::Plain(stream) = ws.get_ref() {
        let _ = stream.set_nodelay(true);
        let _ = stream.set_nonblocking(true);
    } else {
        return fail(metrics, "tls sockets unsupported (ws:// only)".into());
    }

    // 3. Join→ready: descriptor frame first, then deltas through the core.
    let mut core: Option<ClientCore> = None;
    let mut input_fields: Vec<cyg_core_rs::SchemaField> = Vec::new();
    let mut first_full_bytes = 0u64;
    let loop_start = Instant::now();
    loop {
        if loop_start.elapsed() > Duration::from_millis(args.timeout_ms) {
            return fail(
                metrics,
                format!("no full schema snapshot within {} ms", args.timeout_ms),
            );
        }
        match ws.read() {
            Ok(tungstenite::Message::Binary(frame)) => match classify_frame(&frame) {
                Inbound::Descriptor => match ClientCore::new(&frame) {
                    Ok(c) => {
                        input_fields = c
                            .descriptor()
                            .schemas
                            .iter()
                            .find(|s| s.name == cyg_core_rs::INPUT_SCHEMA_NAME)
                            .map(|s| s.fields.clone())
                            .unwrap_or_default();
                        if input_fields.is_empty() {
                            metrics.errors.push(
                                "descriptor has no Input schema — inputs will be skipped".into(),
                            );
                        }
                        core = Some(c);
                    }
                    Err(e) => return fail(metrics, format!("bad descriptor frame: {e}")),
                },
                Inbound::Delta => {
                    let kind = delta_kind(&frame);
                    let bytes_in = frame.len() as u64;
                    let c = match core.as_mut() {
                        Some(c) => c,
                        None => {
                            return fail(metrics, "delta frame before descriptor".into());
                        }
                    };
                    if let Err(e) = c.apply_payload(&frame) {
                        metrics.errors.push(format!("apply_payload: {e}"));
                        continue;
                    }
                    if c.state().ready {
                        // Ack full keyframes or the host keeps re-sending
                        // fulls instead of patches (cyg.js `_ackSchema`).
                        if kind.map(is_full_kind).unwrap_or(false) {
                            first_full_bytes = bytes_in;
                            send_frame(&mut ws, &encode_schema_ready_ack());
                        }
                        let ready_ms = join_start.elapsed().as_secs_f64() * 1000.0;
                        metrics.ready = true;
                        metrics.ready_ms = ready_ms;
                        let phase = JoinedPhase {
                            ws,
                            core: match core.take() {
                                Some(c) => c,
                                None => unreachable!(),
                            },
                            input_fields,
                            seq: 0,
                            first_full_bytes,
                        };
                        return Ok((metrics, phase));
                    }
                }
                Inbound::Other => {}
            },
            Ok(_) => {}
            Err(tungstenite::Error::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => return fail(metrics, format!("ws read failed during join: {e}")),
        }
        std::thread::sleep(Duration::from_micros(200));
    }
}

/// Effective room name for one client (--rooms spreads "room-0".."room-n-1").
fn client_room(index: usize, args: &Args) -> &str {
    if args.room_id.is_some() {
        args.room_id.as_deref().unwrap()
    } else if args.rooms > 1 {
        Box::leak(format!("{}-{}", args.room, index % args.rooms).into_boxed_str())
    } else {
        &args.room
    }
}

fn matchmake(args: &Args, room: &str) -> Result<(Seat, Duration), String> {
    let method = if args.room_id.is_some() { "joinById" } else { "joinOrCreate" };
    let url = format!(
        "{}/api/matchmake/{method}/{room}",
        args.url.trim_end_matches('/')
    );
    let body = r#"{"webrtc":false,"autoReconnect":false}"#;
    let start = Instant::now();
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .build();
    let response = agent
        .post(&url)
        .set("content-type", "application/json")
        .send_string(body)
        .map_err(|e| match e {
            ureq::Error::Status(code, resp) => {
                let text = resp.into_string().unwrap_or_default();
                if code == 429 {
                    "matchmake rate-limited (429) — re-run with --ramp <max joins/sec> \
                     (e.g. --ramp 20)"
                        .to_string()
                } else {
                    format!(
                        "matchmake {method}/{room} failed ({code}): {}",
                        text.trim()
                    )
                }
            }
            other => format!("matchmake transport error: {other}"),
        })?;
    let json: serde_json::Value = response
        .into_json()
        .map_err(|e| format!("matchmake returned invalid JSON: {e}"))?;
    let seat = parse_seat(&json)?;
    Ok((seat, start.elapsed()))
}

fn send_frame(ws: &mut WebSocket<MaybeTlsStream<TcpStream>>, frame: &[u8]) -> bool {
    ws.send(tungstenite::Message::Binary(frame.to_vec())).is_ok()
}

struct WindowReport {
    fulls: u64,
    patches: u64,
    bytes_in: u64,
    bytes_out: u64,
    inputs: u64,
    entities_min: usize,
    entities_max: usize,
    errors: Vec<String>,
}

fn zero_values(fields: &[cyg_core_rs::SchemaField]) -> Result<Vec<InputValue>, String> {
    fields
        .iter()
        .map(|f| match f.kind {
            cyg_core_rs::WireKind::Bool => Ok(InputValue::Bool(false)),
            cyg_core_rs::WireKind::F32 => Ok(InputValue::F32(0.0)),
            cyg_core_rs::WireKind::F64 => Ok(InputValue::F64(0.0)),
            cyg_core_rs::WireKind::U8 => Ok(InputValue::U8(0)),
            cyg_core_rs::WireKind::U16 => Ok(InputValue::U16(0)),
            cyg_core_rs::WireKind::I16 => Ok(InputValue::I16(0)),
            cyg_core_rs::WireKind::U32 => Ok(InputValue::U32(0)),
            cyg_core_rs::WireKind::I32 => Ok(InputValue::I32(0)),
            cyg_core_rs::WireKind::I64 => Ok(InputValue::I64(0)),
            cyg_core_rs::WireKind::I8 => Ok(InputValue::I8(0)),
            other => Err(format!(
                "Input schema field '{}' has unsupported kind {:?}",
                f.name, other
            )),
        })
        .collect()
}

fn measurement_phase(args: &Args, gate: &SharedGate, mut phase: JoinedPhase) -> WindowReport {
    let window_start = gate.wait_for_window();
    let window_deadline = window_start + Duration::from_secs(args.seconds);
    let input_period = Duration::from_nanos(1_000_000_000 / args.hz.max(1) as u64);
    let mut next_input = Instant::now();

    let mut report = WindowReport {
        fulls: 0,
        patches: 0,
        bytes_in: phase.first_full_bytes,
        bytes_out: 0,
        inputs: 0,
        entities_min: usize::MAX,
        entities_max: 0,
        errors: Vec::new(),
    };

    while Instant::now() < window_deadline {
        // Send inputs at --hz through the shared core's encoder.
        let now = Instant::now();
        if !phase.input_fields.is_empty() && now >= next_input {
            phase.seq = phase.seq.wrapping_add(1);
            let values = if args.move_inputs {
                match movement_values(
                    &phase.input_fields,
                    (now - window_start).as_millis() as u64,
                ) {
                    Ok(v) => v,
                    Err(e) => {
                        report.errors.push(e);
                        break;
                    }
                }
            } else {
                match zero_values(&phase.input_fields) {
                    Ok(v) => v,
                    Err(e) => {
                        report.errors.push(e);
                        break;
                    }
                }
            };
            match encode_schema_input_with(&phase.core, phase.seq, &values) {
                Ok(frame) => {
                    if send_frame(&mut phase.ws, &frame) {
                        report.inputs += 1;
                        report.bytes_out += frame.len() as u64;
                    }
                }
                Err(e) => report.errors.push(format!("encode input: {e}")),
            }
            next_input += input_period;
            if next_input < now {
                next_input = now + input_period; // fell behind: skip missed slots
            }
        }

        // Drain inbound deltas through the shared core's decoder.
        match phase.ws.read() {
            Ok(tungstenite::Message::Binary(frame)) => match classify_frame(&frame) {
                Inbound::Delta => {
                    let kind = delta_kind(&frame);
                    report.bytes_in += frame.len() as u64;
                    if kind.map(is_full_kind).unwrap_or(false) {
                        report.fulls += 1;
                        if send_frame(&mut phase.ws, &encode_schema_ready_ack()) {
                            report.bytes_out += 19; // [0x05][17]["__ox_schema_ready"]
                        }
                    } else if kind.map(is_patch_kind).unwrap_or(false) {
                        report.patches += 1;
                    }
                    if let Err(e) = phase.core.apply_payload(&frame) {
                        report.errors.push(format!("apply_payload: {e}"));
                    }
                    let entities = entity_count(&phase.core);
                    report.entities_min = report.entities_min.min(entities);
                    report.entities_max = report.entities_max.max(entities);
                }
                _ => {}
            },
            Ok(_) => {}
            Err(tungstenite::Error::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => {
                report.errors.push(format!("ws read failed: {e}"));
                break;
            }
        }
        std::thread::sleep(Duration::from_micros(200));
    }
    let _ = phase.ws.close(None);
    report.entities_min = if report.entities_min == usize::MAX {
        0
    } else {
        report.entities_min
    };
    report
}

fn entity_count(core: &ClientCore) -> usize {
    (0..core.collection_count())
        .filter_map(|i| core.collection_at(i))
        .map(|c| c.entities.len())
        .sum()
}

fn build_report(
    args: &Args,
    records: &BTreeMap<usize, ClientMetrics>,
    duration_sec: f64,
    requested: usize,
    ready_count: usize,
) -> serde_json::Value {
    let list: Vec<&ClientMetrics> = records.values().collect();
    let joins: Vec<f64> = list.iter().map(|m| m.join_ms).collect();
    let readies: Vec<f64> = list.iter().map(|m| m.ready_ms).collect();
    let total_updates: u64 = list.iter().map(|m| m.fulls_window + m.patches_window).sum();
    let total_fulls: u64 = list.iter().map(|m| m.fulls_window).sum();
    let total_patches: u64 = list.iter().map(|m| m.patches_window).sum();
    let total_in: u64 = list.iter().map(|m| m.bytes_in_window).sum();
    let total_out: u64 = list.iter().map(|m| m.bytes_out_window).sum();
    let total_inputs: u64 = list.iter().map(|m| m.inputs_window).sum();
    let errors: Vec<String> = list.iter().flat_map(|m| m.errors.iter().cloned()).collect();
    let denom = ready_count.max(1) as f64 * duration_sec;
    let entity_min = list.iter().map(|m| m.entities_min).min().unwrap_or(0);
    let entity_max = list.iter().map(|m| m.entities_max).max().unwrap_or(0);

    let r1 = |v: f64| (v * 10.0).round() / 10.0;
    serde_json::json!({
        "engine": "cyg-loadtest(rust)",
        "endpoint": args.url,
        "room": args.room_id.as_deref().map(|_| serde_json::Value::Null)
            .unwrap_or_else(|| serde_json::Value::String(args.room.clone())),
        "roomId": args.room_id,
        "ramp": args.ramp,
        "clients": {
            "requested": requested,
            "joined": ready_count,
            "ready": ready_count,
        },
        "window": {
            "seconds": r1(duration_sec),
            "inputHz": args.hz,
            "move": args.move_inputs,
        },
        "joinMs": {
            "p50": r1(percentile(&joins, 50.0)),
            "p95": r1(percentile(&joins, 95.0)),
            "mean": r1(mean(&joins)),
        },
        "firstSnapshotMs": {
            "p50": r1(percentile(&readies, 50.0)),
            "p95": r1(percentile(&readies, 95.0)),
            "mean": r1(mean(&readies)),
        },
        "updates": {
            "total": total_updates,
            "full": total_fulls,
            "patch": total_patches,
            "perClientPerSecond": r1(total_updates as f64 / denom),
        },
        "traffic": {
            "bytesIn": total_in,
            "bytesOut": total_out,
            "bytesInPerSecond": (total_in as f64 / duration_sec).round() as u64,
            "bytesOutPerSecond": (total_out as f64 / duration_sec).round() as u64,
            "bytesInPerClient": (total_in as f64 / ready_count.max(1) as f64).round() as u64,
            "bytesOutPerClient": (total_out as f64 / ready_count.max(1) as f64).round() as u64,
        },
        "inputs": {
            "sent": total_inputs,
            "perClientPerSecond": r1(total_inputs as f64 / denom),
        },
        "state": {
            "entitiesMin": entity_min,
            "entitiesMax": entity_max,
        },
        "errors": errors,
        "perClient": list.iter().map(|m| serde_json::json!({
            "index": m.index,
            "sessionId": m.session_id,
            "joinMs": r1(m.join_ms),
            "readyMs": r1(m.ready_ms),
            "updates": m.fulls_window + m.patches_window,
            "full": m.fulls_window,
            "patch": m.patches_window,
            "bytesIn": m.bytes_in_window,
            "bytesOut": m.bytes_out_window,
            "inputsSent": m.inputs_window,
            "entities": m.entities_max,
            "errors": m.errors,
        })).collect::<Vec<_>>(),
    })
}

fn print_report(report: &serde_json::Value) {
    let row = |label: &str, value: String| println!("  {label:<15} {value}");
    let f = |o: &serde_json::Value, k: &str| o.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0);
    let u = |o: &serde_json::Value, k: &str| o.get(k).and_then(|v| v.as_u64()).unwrap_or(0);
    println!(
        "CYG load test — {} (room {}{})",
        report["endpoint"].as_str().unwrap_or(""),
        report["room"].as_str().unwrap_or(""),
        if u(&report["ramp"], "") > 0 {
            format!(" · ramp {}/s", u(&report["ramp"], ""))
        } else {
            String::new()
        },
    );
    row(
        "clients",
        format!(
            "{}/{} joined · {} ready",
            u(&report["clients"], "joined"),
            u(&report["clients"], "requested"),
            u(&report["clients"], "ready"),
        ),
    );
    row(
        "join",
        format!(
            "p50 {:.1} ms · p95 {:.1} ms · mean {:.1} ms",
            f(&report["joinMs"], "p50"),
            f(&report["joinMs"], "p95"),
            f(&report["joinMs"], "mean"),
        ),
    );
    row(
        "first snapshot",
        format!(
            "p50 {:.1} ms · p95 {:.1} ms · mean {:.1} ms",
            f(&report["firstSnapshotMs"], "p50"),
            f(&report["firstSnapshotMs"], "p95"),
            f(&report["firstSnapshotMs"], "mean"),
        ),
    );
    row(
        "updates",
        format!(
            "{}/s per client · {} frames ({} full, {} patch)",
            f(&report["updates"], "perClientPerSecond"),
            u(&report["updates"], "total"),
            u(&report["updates"], "full"),
            u(&report["updates"], "patch"),
        ),
    );
    let bytes_fmt = |v: u64| {
        if v < 1024 {
            format!("{v} B")
        } else if v < 1024 * 1024 {
            format!("{:.1} KiB", v as f64 / 1024.0)
        } else {
            format!("{:.2} MiB", v as f64 / 1024.0 / 1024.0)
        }
    };
    row(
        "traffic in",
        format!(
            "{} total · {}/s",
            bytes_fmt(u(&report["traffic"], "bytesIn")),
            bytes_fmt(u(&report["traffic"], "bytesInPerSecond")),
        ),
    );
    row(
        "traffic out",
        format!(
            "{} total · {}/s",
            bytes_fmt(u(&report["traffic"], "bytesOut")),
            bytes_fmt(u(&report["traffic"], "bytesOutPerSecond")),
        ),
    );
    row(
        "inputs",
        format!(
            "{} sent · {}/s per client",
            u(&report["inputs"], "sent"),
            f(&report["inputs"], "perClientPerSecond"),
        ),
    );
    row(
        "state",
        format!(
            "{}–{} entities",
            u(&report["state"], "entitiesMin"),
            u(&report["state"], "entitiesMax"),
        ),
    );
    let errors = report["errors"].as_array().cloned().unwrap_or_default();
    row(
        "errors",
        if errors.is_empty() {
            "0".into()
        } else {
            let first = errors[0].as_str().unwrap_or("").to_string();
            format!("{} ({})", errors.len(), first)
        },
    );
}
