use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::time::{Duration, Instant};

use engine_wasm::WasmRuntime;

use crate::netcode;
use crate::rooms::{self, RoomHostData};

pub(crate) fn live_tick_rate(tick_interval: Duration) -> u32 {
    (1.0 / tick_interval.as_secs_f64()).round().max(1.0) as u32
}

pub(crate) fn input_budget_allows(
    rates: &mut HashMap<std::rc::Rc<str>, netcode::InputRateLimiter>,
    tick_interval: Duration,
    session: &str,
) -> bool {
    input_budget_allows_n(rates, tick_interval, session, 1)
}

/// Charge a whole input batch to the same token bucket. Existing sessions take
/// the borrow-first path; only the first packet interns a key.
pub(crate) fn input_budget_allows_n(
    rates: &mut HashMap<std::rc::Rc<str>, netcode::InputRateLimiter>,
    tick_interval: Duration,
    session: &str,
    samples: usize,
) -> bool {
    let now = Instant::now();
    if let Some(limiter) = rates.get_mut(session) {
        return limiter.allow_n(now, samples);
    }
    rates
        .entry(std::rc::Rc::from(session))
        .or_insert_with(|| netcode::InputRateLimiter::for_tick_rate(live_tick_rate(tick_interval)))
        .allow_n(now, samples)
}

/// Raw-UDP input gate: rate budget first, then the same IN PLACE sanitize
/// the seated lanes get (`sanitize_input_payload` clamps instead of
/// rejecting, so the guest only ever reads legal lanes).
pub(crate) fn udp_input_allowed(
    rates: &mut HashMap<SocketAddr, netcode::InputRateLimiter>,
    tick_interval: Duration,
    peer: SocketAddr,
    sample: &mut [u8; 20],
    domains: &engine_schema::InputDomainMap,
) -> bool {
    let budget = rates
        .entry(peer)
        .or_insert_with(|| netcode::InputRateLimiter::for_tick_rate(live_tick_rate(tick_interval)))
        .allow(Instant::now());
    netcode::sanitize_input_payload(sample, domains);
    budget
}

/// Fail-fast: refuse a guest whose ABI imports a backend the operator's
/// `[features]` table turned off, instead of answering every call with a
/// silent zero at runtime.
pub(crate) fn assert_features_support_module(
    features: &engine_config::FeaturesConfig,
    module: &wasmtime::Module,
) -> Result<(), String> {
    let mut wants2d = false;
    let mut wants3d = false;
    for import in module.imports() {
        let name = import.name();
        if name.contains("rapier2d") {
            wants2d = true;
        } else if name.contains("rapier3d") {
            wants3d = true;
        }
    }
    if wants2d && !features.physics.wants_rapier2d() {
        return Err(
            "[features] physics excludes 2D but the guest imports sys_rapier2d_* —              set [features] physics = \"both\" (or \"2d\") in config.toml, or ship a game that doesn't use 2D physics"
                .to_string(),
        );
    }
    if wants3d && !features.physics.wants_rapier3d() {
        return Err(
            "[features] physics excludes 3D but the guest imports sys_rapier3d_* —              set [features] physics = \"both\" (or \"3d\") in config.toml, or ship a game that doesn't use 3D physics"
                .to_string(),
        );
    }
    Ok(())
}

pub(crate) fn prepare_console_module(
    runtime: &WasmRuntime,
    sha256: &str,
    bytes: &[u8],
    permissions: engine_wasm::AbiPermissions,
    features: &engine_config::FeaturesConfig,
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
    assert_features_support_module(features, &module)?;
    let linker = rooms::build_host_linker(runtime, permissions);
    runtime.prepare(&linker, &module).map_err(|e| e.to_string())
}

pub(crate) fn prepare_file_module(
    runtime: &WasmRuntime,
    wasm_path: &Path,
    permissions: engine_wasm::AbiPermissions,
    features: &engine_config::FeaturesConfig,
) -> Option<engine_wasm::WasmPreInstance<RoomHostData>> {
    let module = runtime
        .load_or_compile_cached(wasm_path)
        .map_err(|e| tracing::info!("[server] load room module: {e}"))
        .ok()?;
    if let Err(e) = assert_features_support_module(features, &module) {
        tracing::error!("[server] ✖ {e}");
        return None;
    }
    let linker = rooms::build_host_linker(runtime, permissions);
    runtime
        .prepare(&linker, &module)
        .map_err(|e| tracing::info!("[server] load room module: {e}"))
        .ok()
}
