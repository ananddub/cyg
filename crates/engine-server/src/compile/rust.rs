use std::path::Path;
use std::process::Command;

pub(crate) fn compile_rust(
    project_dir: &Path,
    src: &Path,
    out: &Path,
    sdk_dir: &Path,
) -> std::io::Result<()> {
    tracing::info!(
        "[build] compiling Rust room '{}' -> '{}' ...",
        src.display(),
        out.display()
    );

    // 1. If Cargo.toml exists, use `cargo build --target wasm32-unknown-unknown --release`
    let cargo_toml = if project_dir.join("Cargo.toml").exists() {
        Some(project_dir.join("Cargo.toml"))
    } else if let Some(parent) = src.parent() {
        if parent.join("Cargo.toml").exists() {
            Some(parent.join("Cargo.toml"))
        } else {
            None
        }
    } else {
        None
    };

    if let Some(cargo_path) = cargo_toml {
        let manifest_dir = cargo_path.parent().unwrap();
        let status = Command::new("cargo")
            .current_dir(manifest_dir)
            .arg("build")
            .arg("--target")
            .arg("wasm32-unknown-unknown")
            .arg("--release")
            .status()?;

        if !status.success() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("cargo build failed with status {status}"),
            ));
        }

        let target_dir = manifest_dir.join("target/wasm32-unknown-unknown/release");
        let mut wasm_file = None;
        if target_dir.exists() {
            for entry in std::fs::read_dir(&target_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().map_or(false, |ext| ext == "wasm") {
                    wasm_file = Some(path);
                    break;
                }
            }
        }
        if let Some(produced) = wasm_file {
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&produced, out)?;
            return Ok(());
        } else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("compiled .wasm not found in {}", target_dir.display()),
            ));
        }
    }

    // 2. Direct rustc single-file compilation
    let rust_sdk_dir = if sdk_dir.join("rust").join("cyg_room.rs").exists() {
        sdk_dir.join("rust")
    } else if project_dir.join("sdk").join("server").join("rust").exists() {
        project_dir.join("sdk").join("server").join("rust")
    } else {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.join("sdk/server/rust"))
            .unwrap_or_else(|| project_dir.join("sdk/server/rust"))
    };

    let mut cmd = Command::new("rustc");
    cmd.arg("--target")
        .arg("wasm32-unknown-unknown")
        .arg("-O")
        .arg("--crate-type")
        .arg("cdylib")
        .arg("-L")
        .arg(&rust_sdk_dir)
        .arg(src)
        .arg("-o")
        .arg(out);

    let output = cmd.output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("rustc compilation failed:\n{stderr}"),
        ));
    }

    Ok(())
}
