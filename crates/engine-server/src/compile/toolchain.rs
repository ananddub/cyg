use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) fn wasi_sdk_bin() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("HOME") {
        let candidate = PathBuf::from(home).join(".cache/tree-sitter/wasi-sdk/bin");
        if candidate.exists() {
            return Some(candidate);
        }
    }
    let system_opt = PathBuf::from("/opt/wasi-sdk/bin");
    if system_opt.exists() {
        return Some(system_opt);
    }
    None
}

pub(crate) fn find_clang() -> Option<PathBuf> {
    if let Some(sdk) = wasi_sdk_bin() {
        let wasi_clang = sdk.join("wasm32-wasip1-clang");
        if wasi_clang.is_file() {
            return Some(wasi_clang);
        }
        let clang = sdk.join("clang");
        if clang.is_file() {
            return Some(clang);
        }
    }
    if which("clang") {
        return Some(PathBuf::from("clang"));
    }
    if which("zig") {
        return Some(PathBuf::from("zig"));
    }
    None
}

pub(crate) fn find_clang_pp() -> Option<PathBuf> {
    if let Some(sdk) = wasi_sdk_bin() {
        let wasi_clang_pp = sdk.join("wasm32-wasi-clang++");
        if wasi_clang_pp.is_file() {
            return Some(wasi_clang_pp);
        }
        let wasip1_clang_pp = sdk.join("wasm32-wasip1-clang++");
        if wasip1_clang_pp.is_file() {
            return Some(wasip1_clang_pp);
        }
        let clang_pp = sdk.join("clang++");
        if clang_pp.is_file() {
            return Some(clang_pp);
        }
    }
    if which("clang++") {
        return Some(PathBuf::from("clang++"));
    }
    None
}

pub(crate) fn run(program: &str, args: &[String]) -> std::io::Result<()> {
    let status = Command::new(program).args(args).status()?;
    if !status.success() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("{program} exited with {status}"),
        ));
    }
    Ok(())
}

pub(crate) fn which(program: &str) -> bool {
    std::env::var("PATH")
        .unwrap_or_default()
        .split(std::path::MAIN_SEPARATOR)
        .any(|dir| {
            let p = Path::new(dir).join(program);
            p.is_file()
        })
}

pub(crate) fn detect_room_class(content: &str) -> Option<String> {
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
