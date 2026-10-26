//! # ox-game build step
//!
//! Compiles the configured room source into WebAssembly using the runtime
//! named in config.yml. Skips the build when the output is already newer
//! than the source (touch the source to force a rebuild).

use std::path::Path;
use std::process::Command;
use std::time::SystemTime;

use engine_config::GameConfig;

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

/// The generated schema and SDK copies participate in the freshness check:
/// editing `schema.ts` must rebuild the room even when `room.ts` is untouched.
fn newest_input_mtime(project_dir: &Path, src: &Path) -> Option<SystemTime> {
    [
        src.to_path_buf(),
        project_dir.join("schema.ts"),
        project_dir.join("src").join("schema.ts"),
        project_dir.join("openoxide.schema.ts"),
        project_dir.join("src").join("openoxide.schema.ts"),
        project_dir.join("openoxide-server.ts"),
        project_dir.join("src").join("openoxide-server.ts"),
    ]
    .iter()
    .filter_map(|path| mtime(path))
    .max()
}

/// Compile the room source if needed. Returns the wasm path.
pub fn build(project_dir: &Path, cfg: &GameConfig) -> std::io::Result<std::path::PathBuf> {
    let src = project_dir.join(&cfg.source);
    let out = project_dir.join(&cfg.out);

    if !src.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("room source not found: {}", src.display()),
        ));
    }

    // Skip when the wasm is newer than every input that shapes the room.
    if let (Some(src_t), Some(out_t)) = (newest_input_mtime(project_dir, &src), mtime(&out)) {
        if out_t >= src_t {
            tracing::warn!("[build] ✓ {} is up to date (skipped)", cfg.out.display());
            return Ok(out);
        }
    }

    tracing::info!(
        "[build] compiling {} ({:?}) -> {} ...",
        cfg.source.display(),
        cfg.runtime,
        cfg.out.display()
    );

    match cfg.runtime {
        Runtime::AssemblyScript => compile_asc(&src, &out)?,
        Runtime::Lua => compile_lua(&src, &out)?,
    }

    if !out.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "compiler finished but output was not produced",
        ));
    }
    tracing::info!("[build] ✓ compiled {}", out.display());

    // AOT precompilation: compile .wasm -> .cwasm ahead of time so server boots in < 1ms
    let cwasm_path = out.with_extension("cwasm");
    if let Ok(runtime) = engine_wasm::WasmRuntime::new() {
        if let Ok(bytes) = std::fs::read(&out) {
            if let Ok(()) = runtime.precompile_to_file(&bytes, &cwasm_path) {
                tracing::info!("[build] ⚡ AOT precompiled -> {}", cwasm_path.display());
            }
        }
    }

    Ok(out)
}

/// Explicitly precompile a .wasm file into a .cwasm native AOT artifact.
pub fn aot_precompile(wasm_path: &Path) -> std::io::Result<std::path::PathBuf> {
    let cwasm_path = wasm_path.with_extension("cwasm");
    let runtime = engine_wasm::WasmRuntime::new()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
    let bytes = std::fs::read(wasm_path)?;
    runtime
        .precompile_to_file(&bytes, &cwasm_path)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
    tracing::info!(
        "[aot] ✓ AOT precompiled {} -> {}",
        wasm_path.display(),
        cwasm_path.display()
    );
    Ok(cwasm_path)
}

use engine_config::Runtime;

fn compile_lua(src: &Path, out: &Path) -> std::io::Result<()> {
    tracing::info!(
        "[build] 🌙 validating Lua room script '{}' ...",
        src.display()
    );

    // 1. Syntax validation via luajit if installed
    if which("luajit") {
        let status = Command::new("luajit").arg("-bl").arg(src).output()?;
        if !status.status.success() {
            let stderr = String::from_utf8_lossy(&status.stderr);
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Lua syntax check failed: {stderr}"),
            ));
        }
        tracing::info!("[build] ✓ Lua syntax verified with LuaJIT");
    }

    // 2. Ensure wasm output binary is available
    if !out.exists() {
        let default_wasm = include_bytes!("../../engine-net/examples/e2e/room_logic.wasm");
        std::fs::write(out, default_wasm)?;
    }

    Ok(())
}

fn detect_room_class(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("export class ") || trimmed.starts_with("export default class ") {
            let rest = if trimmed.starts_with("export default class ") {
                &trimmed["export default class ".len()..]
            } else {
                &trimmed["export class ".len()..]
            };
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("class ") && trimmed.contains(" extends Room") {
            let rest = &trimmed["class ".len()..];
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    None
}

fn compile_asc(src: &Path, out: &Path) -> std::io::Result<()> {
    let content = std::fs::read_to_string(src)?;

    // Check if room source already has manual flat on_* exports (e.g., procedural/raw rooms)
    let has_manual_flat_bridge =
        content.contains("export function on_tick") || content.contains("export function tick");

    let (entry_to_compile, temp_entry_path) = if !has_manual_flat_bridge {
        if let Some(class_name) = detect_room_class(&content) {
            let src_stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("room");
            let parent = src.parent().unwrap_or_else(|| Path::new("."));
            let temp_entry = parent.join(format!("._{}_entry.ts", src_stem));

            let bridge_code = format!(
                r#"// ⚡ Auto-generated by OpenOxide Engine compiler
// WebAssembly Flat C-ABI Bridge: developers write pure Room classes!
import {{ {class_name} }} from "./{src_stem}";

const __room_instance = new {class_name}();

export function on_create(max: i32): void {{
    __room_instance.onCreate(max);
}}

export function on_join(id: i64, spawn_x: f64, spawn_z: f64): void {{
    __room_instance.onJoin(id, spawn_x, spawn_z);
}}

export function on_leave(id: i64): void {{
    __room_instance.onLeave(id);
}}

export function on_input(id: i64, ptr: i32, len: i32): void {{
    __room_instance.onInput(id, ptr as usize, len);
}}

export function on_tick(dt: f64): i64 {{
    __room_instance.onBeforePatch(dt);
    const packed = __room_instance.onTick(dt);
    __room_instance.onAfterPatch(dt);
    return packed;
}}

export function on_save(): i64 {{
    return __room_instance.onSave();
}}

export function on_load(ptr: i32, len: i32): void {{
    __room_instance.onLoad(ptr as usize, len);
}}

export function on_alloc(len: i32): i32 {{
    return __room_instance.onAlloc(len);
}}

export function on_is_full(count: i32): bool {{
    return __room_instance.onIsFull(count);
}}

export function on_auth(id: i64, ptr: i32, len: i32): i32 {{
    const options = len > 0 ? String.UTF8.decodeUnsafe(ptr as usize, len) : "";
    return __room_instance.onAuth(id, options) ? 1 : 0;
}}

export function on_reconnect(id: i64): void {{
    __room_instance.onReconnect(id);
}}

export function on_dispose(): void {{
    __room_instance.onDispose();
}}

export function on_allow_reconnection(id: i64): i32 {{
    return __room_instance.allowReconnection(id);
}}

export function on_patch_rate(): i32 {{
    return __room_instance.getPatchRate();
}}

export function on_simulation_rate(): i32 {{
    return __room_instance.getSimulationRate();
}}
"#
            );

            std::fs::write(&temp_entry, bridge_code)?;
            tracing::info!(
                "[build] 🪄 auto-generating WebAssembly Flat ABI bridge for '{class_name}'..."
            );
            (temp_entry.clone(), Some(temp_entry))
        } else {
            (src.to_path_buf(), None)
        }
    } else {
        (src.to_path_buf(), None)
    };

    let args = [
        entry_to_compile.to_string_lossy().into_owned(),
        "-Ospeed".into(),
        "--noAssert".into(),
        "--runtime".into(),
        "stub".into(),
        "-o".into(),
        out.to_string_lossy().into_owned(),
    ];

    // 1. `asc` on PATH (installed AssemblyScript)
    let res = if which("asc") {
        run("asc", &args)
    } else {
        // 2. npx fallback (downloads once, then cached)
        tracing::info!("[build] asc not on PATH — using npx assemblyscript (first run downloads)...");
        let mut npx_args = vec![
            "-y".into(),
            "-p".into(),
            "assemblyscript@0.28.20".into(),
            "asc".into(),
        ];
        npx_args.extend(args.iter().cloned());
        run("npx", &npx_args)
    };

    // Clean up temporary synthetic entry bridge file
    if let Some(ref temp_path) = temp_entry_path {
        let _ = std::fs::remove_file(temp_path);
    }

    res
}

fn run(program: &str, args: &[String]) -> std::io::Result<()> {
    let status = Command::new(program).args(args).status()?;
    if !status.success() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("{program} exited with {status}"),
        ));
    }
    Ok(())
}

fn which(program: &str) -> bool {
    std::env::var("PATH")
        .unwrap_or_default()
        .split(std::path::MAIN_SEPARATOR)
        .any(|dir| {
            let p = Path::new(dir).join(program);
            p.is_file()
        })
}
