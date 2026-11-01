use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::time::{Duration, Instant};

use engine_wasm::WasmRuntime;

use crate::netcode;
use crate::rooms::{
    self,
    RoomHostData,
};

pub(crate) fn live_tick_rate(tick_interval: Duration) -> u32 {
    (1.0 / tick_interval.as_secs_f64()).round().max(1.0) as u32
}

pub(crate) fn input_budget_allows(
    rates: &mut HashMap<String, netcode::InputRateLimiter>,
    tick_interval: Duration,
    session: &str,
) -> bool {
    rates
        .entry(session.to_string())
        .or_insert_with(|| netcode::InputRateLimiter::for_tick_rate(live_tick_rate(tick_interval)))
        .allow(Instant::now())
}

pub(crate) fn udp_input_allowed(
    rates: &mut HashMap<SocketAddr, netcode::InputRateLimiter>,
    tick_interval: Duration,
    peer: SocketAddr,
    sample: &[u8; 20],
) -> bool {
    let budget = rates
        .entry(peer)
        .or_insert_with(|| netcode::InputRateLimiter::for_tick_rate(live_tick_rate(tick_interval)))
        .allow(Instant::now());
    budget && netcode::validate_input_payload(sample)
}

pub(crate) fn prepare_console_module(
    runtime: &WasmRuntime,
    sha256: &str,
    bytes: &[u8],
    permissions: engine_wasm::AbiPermissions,
) -> Result<engine_wasm::WasmPreInstance<RoomHostData>, String> {
    use sha2::{Digest, Sha256};

    let digest: String = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if !digest.eq_ignore_ascii_case(sha256) {
        return Err(format!(
            "sha256 mismatch: the bytes hash to {digest}, the row says {sha256}"
        ));
    }
    let module = runtime.compile(bytes).map_err(|e| e.to_string())?;
    let linker = rooms::build_host_linker(runtime, permissions);
    runtime.prepare(&linker, &module).map_err(|e| e.to_string())
}

pub(crate) fn prepare_file_module(
    runtime: &WasmRuntime,
    wasm_path: &Path,
    permissions: engine_wasm::AbiPermissions,
) -> Option<engine_wasm::WasmPreInstance<RoomHostData>> {
    let module = runtime
        .load_or_compile_cached(wasm_path)
        .map_err(|e| tracing::info!("[server] load room module: {e}"))
        .ok()?;
    let linker = rooms::build_host_linker(runtime, permissions);
    runtime
        .prepare(&linker, &module)
        .map_err(|e| tracing::info!("[server] load room module: {e}"))
        .ok()
}
