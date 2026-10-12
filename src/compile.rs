//! # ox-game build step
//!
//! Compiles the configured room source into WebAssembly using the runtime
//! named in config.yml. Skips the build when the output is already newer
//! than the source (touch the source to force a rebuild).

use std::path::Path;
use std::process::Command;
use std::time::SystemTime;

use crate::config::GameConfig;

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
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

    // Skip when the wasm is newer than the source.
    if let (Some(src_t), Some(out_t)) = (mtime(&src), mtime(&out)) {
        if out_t >= src_t {
            eprintln!("[build] ✓ {} is up to date (skipped)", cfg.out.display());
            return Ok(out);
        }
    }

    eprintln!(
        "[build] compiling {} ({:?}) -> {} ...",
        cfg.source.display(),
        cfg.runtime,
        cfg.out.display()
    );

    match cfg.runtime {
        Runtime::AssemblyScript => compile_asc(&src, &out)?,
    }

    if !out.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "compiler finished but output was not produced",
        ));
    }
    eprintln!("[build] ✓ compiled {}", out.display());
    Ok(out)
}

use crate::config::Runtime;

fn compile_asc(src: &Path, out: &Path) -> std::io::Result<()> {
    let args = [
        src.to_string_lossy().into_owned(),
        "-Ospeed".into(),
        "--noAssert".into(),
        "--runtime".into(),
        "stub".into(),
        "-o".into(),
        out.to_string_lossy().into_owned(),
    ];

    // 1. `asc` on PATH (installed AssemblyScript)
    if which("asc") {
        return run("asc", &args);
    }

    // 2. npx fallback (downloads once, then cached)
    eprintln!("[build] asc not on PATH — using npx assemblyscript (first run downloads)...");
    let mut npx_args = vec![
        "-y".into(),
        "-p".into(),
        "assemblyscript@0.28.20".into(),
        "asc".into(),
    ];
    npx_args.extend(args.iter().cloned());
    run("npx", &npx_args)
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
