use std::path::Path;
use std::process::Command;

use crate::compile::toolchain::{find_clang, wasi_sdk_bin};

pub(crate) fn compile_c(
    project_dir: &Path,
    src: &Path,
    out: &Path,
    sdk_dir: &Path,
) -> std::io::Result<()> {
    tracing::info!(
        "[build] compiling C room '{}' -> '{}' ...",
        src.display(),
        out.display()
    );

    let c_include = if sdk_dir.join("c").join("cyg.h").exists() {
        sdk_dir.join("c")
    } else if project_dir.join("sdk").join("server").join("c").exists() {
        project_dir.join("sdk").join("server").join("c")
    } else {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.join("sdk/server/c"))
            .unwrap_or_else(|| project_dir.join("sdk/server/c"))
    };

    let clang = find_clang().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "C compiler not found: please install clang or wasi-sdk",
        )
    })?;

    let is_zig = clang.to_string_lossy().contains("zig");
    let mut cmd = if is_zig {
        let mut c = Command::new(&clang);
        c.arg("cc")
            .arg("-target")
            .arg("wasm32-freestanding")
            .arg("-O3")
            .arg("-shared")
            .arg("-Wl,--no-entry")
            .arg("-Wl,--export-dynamic")
            .arg("-Wl,--allow-undefined");
        c
    } else {
        let mut c = Command::new(&clang);
        c.arg("--target=wasm32")
            .arg("-O3")
            .arg("-nostdlib")
            .arg("-Wl,--no-entry")
            .arg("-Wl,--export-dynamic")
            .arg("-Wl,--allow-undefined");
        c
    };

    if let Some(bin_dir) = wasi_sdk_bin() {
        let current_path = std::env::var("PATH").unwrap_or_default();
        cmd.env("PATH", format!("{}:{}", bin_dir.display(), current_path));
    }

    if c_include.exists() {
        cmd.arg("-I").arg(&c_include);
        if let Some(parent) = c_include.parent() {
            cmd.arg("-I").arg(parent);
            if let Some(grand) = parent.parent() {
                cmd.arg("-I").arg(grand);
            }
        }
    }
    cmd.arg("-I").arg(project_dir);
    cmd.arg(src).arg("-o").arg(out);

    let output = cmd.output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("C compilation failed:\n{stderr}"),
        ));
    }

    Ok(())
}
