//! # ox-game — OpenOxide game server CLI
//!
//!   ox-game init [dir]    create a game project (config.yml + room.ts + website)
//!   ox-game run  [dir]    compile (if needed) and run the server
//!   ox-game run           (with no config.yml present) runs the built-in demo
//!
//! Everything is driven by config.yml — runtime, source, ports, tick rate.
//!
//! The engine itself lives in library crates: `engine-bus` (plane wiring),
//! `engine-config` (config.toml), `engine-control` (Tokio/Axum control plane)
//! and `engine-server` (rooms, netcode, sim plane, host loop, wasm build).

pub use engine_protocol as net_sync;
pub use engine_physics as physics;
pub use engine_physics::rapier as rapier_integration;
pub use engine_spatial as spatial;

use engine_config::{GameConfig, CONFIG_TEMPLATE, ROOM_TEMPLATE};
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
        "gen-interface" | "interfaces" | "sdk" => {
            if let Err(e) = generate_interfaces(&dir) {
                eprintln!("✗ gen-interface failed: {e}");
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
  ox-game init          [dir]    create a game project (config.yml + room.ts + website.html)
  ox-game run           [dir]    compile the room (if needed) and start the server
  ox-game gen-interface [dir]    auto-generate C++, Rust, C#, AssemblyScript & TypeScript interfaces
  ox-game                        same as `run` in the current directory

CONFIG:
  Everything lives in config.toml — runtime, source file, ports, tick rate.
  See the template `ox-game init` writes."
    );
}

/// `ox-game init [dir]` — scaffold a project.
fn init(dir: &Path) -> std::io::Result<()> {
    let config_path = dir.join("config.toml");
    if config_path.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("{} already exists — not overwriting", config_path.display()),
        ));
    }
    std::fs::create_dir_all(dir)?;
    std::fs::write(&config_path, CONFIG_TEMPLATE)?;
    std::fs::write(dir.join("room.ts"), ROOM_TEMPLATE)?;
    std::fs::write(
        dir.join("website.html"),
        include_bytes!("../crates/engine-net/examples/e2e/website.html"),
    )?;

    println!("✓ game project created in {}", dir.display());
    println!();
    println!(
        "  {}/config.toml     — runtime, ports, tick rate",
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
    let cfg = if dir.join("config.toml").exists() || dir.join("config.yml").exists() || dir.is_file() {
        engine_config::load_config(dir)?
    } else if dir == Path::new(".") && !Path::new("config.toml").exists() && !Path::new("config.yml").exists() {
        // No project here: run the built-in demo (example room + website).
        eprintln!("[run] no config.toml found — running the built-in demo");
        GameConfig {
            name: "game_room".into(),
            source: PathBuf::from("crates/engine-net/examples/e2e/room.ts"),
            out: PathBuf::new(), // pre-compiled wasm used below
            ..GameConfig::default()
        }
    } else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("config.toml not found in {}", dir.display()),
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
    let wasm_path = engine_server::compile::build(&project_dir, &cfg)?;

    // One bus for both planes: the sim thread owns the consumer ends, the
    // control plane only ever `try_send`s onto the producer ends, and the host
    // loop is a `status_tx` producer — it is the default game path, so for most
    // projects it is the only thing that ever publishes telemetry.
    let bus = engine_bus::Bus::new(4096, 8192);

    // Optional raw-UDP + pinned 128 Hz sim plane (Agent 3). Gated by
    // OX_PLANES=1 so the existing WS/WebRTC flow is untouched by default.
    let mut plane_cfg = engine_server::plane::PlaneConfig::from_env(&project_dir);
    plane_cfg.wasm_path = wasm_path.clone();
    if plane_cfg.enabled {
        let udp_addr = plane_cfg.udp_addr.clone();
        match engine_server::plane::spawn_sim_plane(bus.clone(), plane_cfg) {
            Ok(_) => eprintln!("[main] 🧵 sim plane spawned (OX_PLANES=1, udp {udp_addr})"),
            Err(e) => eprintln!("[main] ✖ sim plane spawn failed: {e}"),
        }
    }

    // The Tokio/Axum control plane (Agent 1): matchmake, seat tokens, WS
    // signaling, /status and statics. Bound before `host::run` starts so a
    // port clash is reported here rather than swallowing the game loop.
    // Held for the whole run — dropping it closes every HTTP and socket.
    let _control = if cfg.control.enabled {
        match engine_control::spawn(&project_dir, &cfg, &cfg.control, bus.clone()) {
            Ok(plane) => {
                eprintln!(
                    "[main] 🏛️  control plane on http://{}:{} (matchmake, /ws/signal, /status, statics)",
                    cfg.control.bind_addr, plane.port
                );
                Some(plane)
            }
            Err(e) => {
                eprintln!(
                    "[main] ✖ control plane on port {} failed to start: {e}",
                    cfg.control.api_port
                );
                None
            }
        }
    } else {
        eprintln!("[main] control plane disabled ([control] enabled = false)");
        None
    };

    engine_server::host::run(&project_dir, &wasm_path, &cfg, bus)
}

/// `ox-game gen-interface [dir]` — auto-generate Lua, C, C++, Rust, C#, AssemblyScript & TypeScript room interfaces.
fn generate_interfaces(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;

    let lua_path = dir.join("openoxide.lua");
    let c_path = dir.join("openoxide.h");
    let cpp_path = dir.join("openoxide.hpp");
    let rust_path = dir.join("openoxide_room.rs");
    let csharp_path = dir.join("OpenOxide.cs");
    let as_path = dir.join("openoxide.ts");
    let ts_path = dir.join("openoxide.d.ts");
    let readme_path = dir.join("README.md");

    std::fs::write(&lua_path, include_str!("../sdk/interfaces/lua/openoxide.lua"))?;
    std::fs::write(&c_path, include_str!("../sdk/interfaces/c/openoxide.h"))?;
    std::fs::write(&cpp_path, include_str!("../sdk/interfaces/cpp/openoxide.hpp"))?;
    std::fs::write(&rust_path, include_str!("../sdk/interfaces/rust/openoxide_room.rs"))?;
    std::fs::write(&csharp_path, include_str!("../sdk/interfaces/csharp/OpenOxide.cs"))?;
    std::fs::write(&as_path, include_str!("../sdk/interfaces/assemblyscript/openoxide.ts"))?;
    std::fs::write(&ts_path, include_str!("../sdk/interfaces/typescript/openoxide.d.ts"))?;
    std::fs::write(&readme_path, include_str!("../sdk/interfaces/README.md"))?;

    println!("✓ Successfully auto-generated OpenOxide Room Interfaces in {}:", dir.display());
    println!("  • {} (Lua Room SDK & Class)", lua_path.display());
    println!("  • {} (Pure C Procedural Header)", c_path.display());
    println!("  • {} (C++ Class & Macro Header)", cpp_path.display());
    println!("  • {} (Rust Guest Trait)", rust_path.display());
    println!("  • {} (C# .NET 8 Room Class)", csharp_path.display());
    println!("  • {} (AssemblyScript Room Base Class)", as_path.display());
    println!("  • {} (TypeScript Lifecycle Contract)", ts_path.display());
    println!("  • {} (Quick Start Guide)", readme_path.display());
    Ok(())
}
