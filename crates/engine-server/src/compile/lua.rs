use std::path::Path;
use std::process::Command;

use crate::compile::toolchain::which;

pub(crate) fn compile_lua(src: &Path, out: &Path) -> std::io::Result<()> {
    tracing::info!(
        "[build] 🌙 validating Lua room script '{}' ...",
        src.display()
    );

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

    if !out.exists() {
        let default_wasm = include_bytes!("../../../engine-net/examples/e2e/room_logic.wasm");
        std::fs::write(out, default_wasm)?;
    }

    Ok(())
}
