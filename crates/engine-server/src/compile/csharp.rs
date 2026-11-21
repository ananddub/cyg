use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) fn compile_csharp(
    _project_dir: &Path,
    src: &Path,
    out: &Path,
    _sdk_dir: &Path,
) -> std::io::Result<()> {
    tracing::info!(
        "[build] compiling C# room '{}' -> '{}' ...",
        src.display(),
        out.display()
    );

    // If source itself is already a .wasm file, copy directly
    if src.extension().map_or(false, |ext| ext == "wasm") {
        std::fs::copy(src, out)?;
        return Ok(());
    }

    // Locate .csproj file
    let csproj = if src.extension().map_or(false, |ext| ext == "csproj") {
        Some(src.to_path_buf())
    } else if let Some(parent) = src.parent() {
        let mut found = None;
        if let Ok(entries) = std::fs::read_dir(parent) {
            for entry in entries.flatten() {
                if entry.path().extension().map_or(false, |ext| ext == "csproj") {
                    found = Some(entry.path());
                    break;
                }
            }
        }
        found
    } else {
        None
    };

    if let Some(csproj_path) = csproj {
        let status = Command::new("dotnet")
            .arg("publish")
            .arg(&csproj_path)
            .arg("-c")
            .arg("Release")
            .status()?;

        if !status.success() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("dotnet publish failed with status {status}"),
            ));
        }

        let bin_dir = csproj_path.parent().unwrap().join("bin");
        let mut wasm_file: Option<PathBuf> = None;
        fn find_wasm(dir: &Path, result: &mut Option<PathBuf>) {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        find_wasm(&path, result);
                    } else if path.extension().map_or(false, |ext| ext == "wasm") {
                        *result = Some(path);
                        return;
                    }
                }
            }
        }
        find_wasm(&bin_dir, &mut wasm_file);

        if let Some(found_wasm) = wasm_file {
            std::fs::copy(&found_wasm, out)?;
            return Ok(());
        } else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!(
                    "dotnet build completed but .wasm file was not found under {}",
                    bin_dir.display()
                ),
            ));
        }
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        format!("No .csproj found to compile C# room for {}", src.display()),
    ))
}
