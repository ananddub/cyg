//! # ox-game — OpenOxide game server CLI
//!
//!   ox-game init [dir]    create a game project (config.yml + room.ts + website)
//!   ox-game run  [dir]    compile (if needed) and run the server
//!   ox-game run           (with no config.yml present) runs the built-in demo
//!
//! Everything is driven by config.yml — runtime, source, ports, tick rate.

mod compile;
mod config;
mod server;

use config::GameConfig;
use std::path::{Path, PathBuf};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("run");
    let dir = args
        .get(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    match cmd {
        "init" => {
            if let Err(e) = init(&dir) {
                eprintln!("✗ init failed: {e}");
                std::process::exit(1);
            }
        }
        "run" => {
            if let Err(e) = run(&dir) {
                eprintln!("✗ run failed: {e}");
                std::process::exit(1);
            }
        }
        "--help" | "-h" | "help" => print_help(),
        other => {
            eprintln!("unknown command: {other}");
            print_help();
            std::process::exit(1);
        }
    }
}

fn print_help() {
    println!(
        "ox-game — OpenOxide game server

USAGE:
  ox-game init [dir]    create a game project (config.yml + room.ts + website.html)
  ox-game run  [dir]    compile the room (if needed) and start the server
  ox-game               same as `run` in the current directory

CONFIG:
  Everything lives in config.yml — runtime, source file, ports, tick rate.
  See the template `ox-game init` writes."
    );
}

/// `ox-game init [dir]` — scaffold a project.
fn init(dir: &Path) -> std::io::Result<()> {
    let config_path = dir.join("config.yml");
    if config_path.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("{} already exists — not overwriting", config_path.display()),
        ));
    }
    std::fs::create_dir_all(dir)?;
    std::fs::write(&config_path, config::CONFIG_TEMPLATE)?;
    std::fs::write(dir.join("room.ts"), config::ROOM_TEMPLATE)?;
    std::fs::write(
        dir.join("website.html"),
        include_bytes!("../crates/engine-net/examples/e2e/website.html"),
    )?;

    println!("✓ game project created in {}", dir.display());
    println!();
    println!(
        "  {}/config.yml     — runtime, ports, tick rate",
        dir.display()
    );
    println!(
        "  {}/room.ts        — your room logic (edit me!)",
        dir.display()
    );
    println!("  {}/website.html   — the canvas page", dir.display());
    println!();
    println!("next: ox-game run {}", dir.display());
    Ok(())
}

/// `ox-game run [dir]` — compile (if needed) and serve.
fn run(dir: &Path) -> std::io::Result<()> {
    let cfg = if dir.join("config.yml").exists() || dir.join("config.yml").is_file() {
        config::load_config(dir)?
    } else if dir == Path::new(".") && !Path::new("config.yml").exists() {
        // No project here: run the built-in demo (example room + website).
        eprintln!("[run] no config.yml found — running the built-in demo");
        GameConfig {
            name: "game_room".into(),
            source: PathBuf::from("crates/engine-net/examples/e2e/room.ts"),
            out: PathBuf::new(), // pre-compiled wasm used below
            ..GameConfig::default()
        }
    } else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("config.yml not found in {}", dir.display()),
        ));
    };

    // Demo mode uses the pre-compiled example wasm shipped with the repo.
    let (project_dir, cfg) = if cfg.out.as_os_str().is_empty() {
        let wasm = include_bytes!("../crates/engine-net/examples/e2e/room_logic.wasm");
        let out_dir = std::env::temp_dir().join("ox-game-demo");
        std::fs::create_dir_all(&out_dir)?;
        let out = out_dir.join("room_logic.wasm");
        std::fs::write(&out, wasm)?;
        (PathBuf::from("."), GameConfig { out: out, ..cfg })
    } else {
        (dir.to_path_buf(), cfg.clone())
    };

    println!("🧩 room runtime: {:?}", cfg.runtime);
    let wasm_path = compile::build(&project_dir, &cfg)?;
    server::run(&project_dir, &wasm_path, &cfg)
}
