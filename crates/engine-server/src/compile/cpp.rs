use std::path::Path;
use std::process::Command;

use crate::compile::toolchain::{find_clang_pp, wasi_sdk_bin};

pub(crate) fn compile_cpp(
    project_dir: &Path,
    src: &Path,
    out: &Path,
    sdk_dir: &Path,
) -> std::io::Result<()> {
    tracing::info!(
        "[build] compiling C++ room '{}' -> '{}' ...",
        src.display(),
        out.display()
    );

    let cpp_include = if sdk_dir.join("cpp").join("cyg.hpp").exists() {
        sdk_dir.join("cpp")
    } else if project_dir.join("sdk").join("server").join("cpp").exists() {
        project_dir.join("sdk").join("server").join("cpp")
    } else {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.join("sdk/server/cpp"))
            .unwrap_or_else(|| project_dir.join("sdk/server/cpp"))
    };

    let clang_pp = find_clang_pp().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "C++ compiler not found: please install clang++ or wasi-sdk",
        )
    })?;

    let mut cmd = Command::new(&clang_pp);
    if let Some(bin_dir) = wasi_sdk_bin() {
        let current_path = std::env::var("PATH").unwrap_or_default();
        cmd.env("PATH", format!("{}:{}", bin_dir.display(), current_path));
    }

    cmd.arg("--target=wasm32")
        .arg("-O3")
        .arg("-nostdlib")
        .arg("-fno-exceptions")
        .arg("-fno-rtti")
        .arg("-Wl,--no-entry")
        .arg("-Wl,--export-dynamic")
        .arg("-Wl,--allow-undefined");

    if cpp_include.exists() {
        cmd.arg("-I").arg(&cpp_include);
        if let Some(parent) = cpp_include.parent() {
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
            format!("C++ compilation failed:\n{stderr}"),
        ));
    }

    Ok(())
}
