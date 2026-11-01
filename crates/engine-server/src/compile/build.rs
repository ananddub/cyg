use std::path::Path;
use std::time::SystemTime;

use engine_config::GameConfig;
use engine_config::Runtime;

use crate::compile::asc::compile_asc;
use crate::compile::lua::compile_lua;

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

fn newest_input_mtime(project_dir: &Path, src: &Path) -> Option<SystemTime> {
    [
        src.to_path_buf(),
        project_dir.join("schema.ts"),
        project_dir.join("src").join("schema.ts"),
        project_dir.join("cyg.schema.ts"),
        project_dir.join("src").join("cyg.schema.ts"),
        project_dir.join("cyg-server.ts"),
        project_dir.join("src").join("cyg-server.ts"),
    ]
    .iter()
    .filter_map(|path| mtime(path))
    .max()
}

pub fn build(project_dir: &Path, cfg: &GameConfig) -> std::io::Result<std::path::PathBuf> {
    let src = project_dir.join(&cfg.source);
    let out = project_dir.join(&cfg.out);

    if !src.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("room source not found: {}", src.display()),
        ));
    }

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
