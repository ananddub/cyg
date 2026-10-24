//! # ox-game — OpenOxide game server CLI
//!
//!   ox-game init \[dir]    create a game project (room.ts + schema.ts + own SDK + website)
//!   ox-game run  \[dir]    compile (if needed) and run the server
//!   ox-game run            (with no config.toml present) runs the built-in demo
//!
//! Everything is driven by config.toml — runtime, source, ports, tick rate.
//!
//! The engine itself lives in library crates: `engine-bus` (plane wiring),
//! `engine-config` (config.toml), `engine-api` (the browser-facing HTTP/WS
//! surface), `engine-control` (the Tokio runtime that hosts it) and
//! `engine-server` (rooms, netcode, sim plane, host loop, wasm build).

pub use engine_physics as physics;
pub use engine_physics::rapier as rapier_integration;
pub use engine_spatial as spatial;

use engine_config::{CONFIG_TEMPLATE, GameConfig, ROOM_TEMPLATE, SCHEMA_TS_TEMPLATE};
use engine_schema::codegen::Language;
use engine_schema::{
    FIELD_OPTIONAL, FIELD_STATIC, FIELD_TRANSIENT, SchemaDef, SchemaField, WireKind,
    parse_descriptor, wire_hash,
};
use std::path::{Path, PathBuf};

mod cook;
mod schema;

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
        "aot" | "build" => {
            if let Err(e) = aot(&dir) {
                eprintln!("✗ aot build failed: {e}");
                std::process::exit(1);
            }
        }
        "gen-interface" | "interfaces" | "sdk" => {
            if let Err(e) = generate_interfaces(&dir) {
                eprintln!("✗ gen-interface failed: {e}");
                std::process::exit(1);
            }
        }
        "schema" | "gen-schema" => schema_command(&args[1..]),
        "doctor" => {
            if let Err(e) = doctor(&dir) {
                eprintln!("✗ doctor found problems");
                std::process::exit(1);
            }
        }
        "schema-codegen" | "schema-gen" => schema_codegen_command(&args[1..]),
        "schema-debug" | "schema-inspect" => schema_debug_command(&args[1..]),
        "cook" => {
            if args.len() < 3 {
                eprintln!("Usage: ox-game cook <input.glb> <output.bin>");
                std::process::exit(1);
            }
            let input = Path::new(&args[1]);
            let output = Path::new(&args[2]);
            if let Err(e) = cook::cook_glb(input, output) {
                eprintln!("✗ cook failed: {e}");
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

/// `ox-game doctor [dir]` — one command that validates everything a project
/// needs: config, schema, wasm presence and the exact ABI export names the
/// host will look up. Catches contract mismatches (e.g. `tick()` vs
/// `on_tick()`) in seconds instead of at room-join time.
fn doctor(dir: &Path) -> std::io::Result<()> {
    let mut problems = 0usize;
    println!("🩺 ox-game doctor — {}\n", dir.display());

    // 1. config
    let cfg = match engine_config::load_config(dir) {
        Ok(cfg) => {
            println!("✓ config.toml parses (name '{}', tick {}Hz)", cfg.name, cfg.tick_rate);
            Some(cfg)
        }
        Err(e) => {
            println!("✗ config.toml: {e}");
            problems += 1;
            None
        }
    };

    // 2. schema
    match schema::load_contract(dir) {
        Ok(contract) => println!(
            "✓ schema.ts parses ({} schemas, root '{}')",
            contract.schemas.len(),
            contract.root.name
        ),
        Err(e) => {
            println!("✗ schema: {e}");
            problems += 1;
        }
    }

    // 3. room wasm + export names
    let required = [
        "on_create", "on_join", "on_leave", "on_alloc", "on_input", "on_tick", "on_save", "on_load",
    ];
    let out = cfg.as_ref().map(|c| dir.join(&c.out));
    let wasm_path = out.filter(|p| p.exists());
    match wasm_path {
        Some(path) => {
            let bytes = std::fs::read(&path)?;
            match engine_wasm::WasmRuntime::new()
                    .map_err(|e| e.to_string())
                    .and_then(|r| r.compile(&bytes).map_err(|e| e.to_string()))
                {
                Ok(module) => {
                    let exports: Vec<String> = module.exports().map(|e| e.name().to_string()).collect();
                    let missing: Vec<&str> = required.iter().filter(|r| !exports.contains(&r.to_string())).copied().collect();
                    if missing.is_empty() {
                        println!("✓ room wasm exports the full host ABI ({} exports)", exports.len());
                    } else {
                        println!(
                            "✗ room wasm is missing host exports: {}. The host looks up exactly these names (see sdk/interfaces). Got: {}",
                            missing.join(", "),
                            exports.join(", ")
                        );
                        problems += 1;
                    }
                }
                Err(e) => {
                    println!("✗ room wasm does not compile: {e}");
                    problems += 1;
                }
            }
        }
        None => println!("• room wasm not built yet (run: ox-game run {})", dir.display()),
    }

    if problems > 0 {
        Err(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{problems} problem(s)")))
    } else {
        println!("\n✓ project looks healthy");
        Ok(())
    }
}

fn print_help() {
    println!(
        "ox-game — OpenOxide game server

USAGE:
  ox-game init          [dir]    create a game project (room.ts + schema.ts + own SDK + website)
  ox-game run           [dir]    compile the room (if needed) and start the server
  ox-game aot           [dir]    AOT precompile room to native .cwasm for <1ms cold starts
  ox-game gen-interface [dir]    auto-generate C++, Rust, C#, AssemblyScript & TypeScript interfaces
  ox-game cook <in.glb> <out.bin> extract 3D collision geometry to instant-boot OXCL binary
  ox-game schema [dir] --dump     compile schema.ts and print the descriptor as hex
  ox-game schema [dir] --json     compile schema.ts and print a machine-readable report
  ox-game schema        [dir]    compile schema.ts to server code + browser descriptor
  ox-game schema-codegen [dir]   emit typed client bindings (--lang cs|gdscript|cpp|lua|all)
  ox-game schema-debug [path]    inspect a compiled descriptor (--json)
  ox-game doctor       [dir]    validate config, schema and wasm ABI exports
  ox-game                        same as `run` in the current directory

CONFIG:
  Everything lives in config.toml — runtime, source file, ports, tick rate.
  See the template `ox-game init` writes."
    );
}

fn schema_command(args: &[String]) {
    let mut dir = PathBuf::from(".");
    let mut dump = false;
    let mut json = false;
    for arg in args {
        match arg.as_str() {
            "--dump" => dump = true,
            "--json" => json = true,
            "--help" | "-h" => {
                println!(
                    "usage: ox-game schema [dir] [--dump] [--json]\n\
                     \x20 --dump   print the binary descriptor as lowercase hex\n\
                     \x20 --json   print paths, byte count and descriptor hex as JSON"
                );
                return;
            }
            other if other.starts_with('-') => {
                eprintln!("✗ unknown schema option: {other}");
                std::process::exit(1);
            }
            other if dir == Path::new(".") => dir = PathBuf::from(other),
            other => {
                eprintln!("✗ unexpected schema argument: {other}");
                std::process::exit(1);
            }
        }
    }

    match schema::require_project(&dir) {
        Ok(output) => {
            if let Err(e) = print_schema_output(&output, dump, json) {
                eprintln!("✗ schema report failed: {e}");
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("✗ schema generation failed: {e}");
            std::process::exit(1);
        }
    }
}

fn schema_codegen_command(args: &[String]) {
    let mut dir = PathBuf::from(".");
    let mut out: Option<PathBuf> = None;
    let mut lang = String::from("all");
    let mut check = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--lang" => {
                index += 1;
                match args.get(index) {
                    Some(value) => lang = value.clone(),
                    None => {
                        eprintln!("✗ --lang needs a value (cs, gdscript, cpp, lua or all)");
                        std::process::exit(1);
                    }
                }
            }
            "--out" => {
                index += 1;
                match args.get(index) {
                    Some(value) => out = Some(PathBuf::from(value)),
                    None => {
                        eprintln!("✗ --out needs a directory");
                        std::process::exit(1);
                    }
                }
            }
            "--check" => check = true,
            "--help" | "-h" => {
                println!(
                    "usage: ox-game schema-codegen [dir] [--lang <cs|gdscript|cpp|lua|all>] [--out DIR] [--check]\n\
                     \x20 --lang    target language; defaults to all\n\
                     \x20 --out     output directory; defaults to <dir>/generated\n\
                     \x20 --check   fail if generated files are missing or stale (CI)"
                );
                return;
            }
            other if other.starts_with('-') => {
                eprintln!("✗ unknown schema-codegen option: {other}");
                std::process::exit(1);
            }
            other if dir == Path::new(".") => dir = PathBuf::from(other),
            other => {
                eprintln!("✗ unexpected schema-codegen argument: {other}");
                std::process::exit(1);
            }
        }
        index += 1;
    }

    let languages: Vec<Language> = if lang == "all" {
        Language::ALL.to_vec()
    } else {
        match Language::parse(&lang) {
            Some(language) => vec![language],
            None => {
                eprintln!("✗ unknown language '{lang}'; use cs, gdscript, cpp, lua or all");
                std::process::exit(1);
            }
        }
    };

    let contract = match schema::load_contract(&dir) {
        Ok(contract) => contract,
        Err(e) => {
            eprintln!("✗ schema-codegen failed: {e}");
            std::process::exit(1);
        }
    };
    let out_dir = out.unwrap_or_else(|| dir.join("generated"));

    if check {
        let mut stale = Vec::new();
        for language in languages {
            let path = out_dir.join(language.file_name());
            let expected = schema::codegen_contents(&contract, language);
            let current = std::fs::read_to_string(&path).ok();
            if current.as_deref() != Some(expected.as_str()) {
                stale.push(path);
            } else {
                println!(
                    "✓ {} up to date {}",
                    language.display_name(),
                    path.display()
                );
            }
        }
        if !stale.is_empty() {
            for path in stale {
                eprintln!("✗ stale {}", path.display());
            }
            eprintln!(
                "run `ox-game schema-codegen {} --out {}` to regenerate",
                dir.display(),
                out_dir.display()
            );
            std::process::exit(1);
        }
        return;
    }

    for language in languages {
        match schema::write_codegen(&out_dir, &contract, language) {
            Ok((path, changed)) => println!(
                "✓ {} {} {}",
                language.display_name(),
                if changed { "wrote" } else { "unchanged" },
                path.display()
            ),
            Err(e) => {
                eprintln!("✗ schema-codegen failed: {e}");
                std::process::exit(1);
            }
        }
    }
}

fn schema_debug_command(args: &[String]) {
    let mut path = PathBuf::from(".");
    let mut json = false;
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            "--help" | "-h" => {
                println!(
                    "usage: ox-game schema-debug [dir|file] [--json]\n\
                     \x20 Reads openoxide.schema.bin (or the given file) and prints the\n\
                     \x20 root, collections, schemas, fields and recomputed hashes."
                );
                return;
            }
            other if other.starts_with('-') => {
                eprintln!("✗ unknown schema-debug option: {other}");
                std::process::exit(1);
            }
            other if path == Path::new(".") => path = PathBuf::from(other),
            other => {
                eprintln!("✗ unexpected schema-debug argument: {other}");
                std::process::exit(1);
            }
        }
    }

    let descriptor_path = if path.is_dir() {
        path.join(schema::DESCRIPTOR_FILE)
    } else {
        path.clone()
    };
    let bytes = match std::fs::read(&descriptor_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("✗ cannot read {}: {e}", descriptor_path.display());
            std::process::exit(1);
        }
    };
    let Some(descriptor) = parse_descriptor(&bytes) else {
        eprintln!(
            "✗ {} is not a valid OXSD descriptor ({} bytes)",
            descriptor_path.display(),
            bytes.len()
        );
        std::process::exit(1);
    };

    let root_hash = if descriptor.version >= 2 {
        let root_schema = SchemaDef {
            name: descriptor.root.name.clone(),
            version: descriptor.root.version,
            fields: descriptor.root.scalars.clone(),
            line: 0,
            column: 0,
        };
        Some(wire_hash(&root_schema))
    } else {
        None
    };

    if json {
        print_schema_debug_json(&descriptor_path, bytes.len(), &descriptor, root_hash);
        return;
    }

    println!(
        "descriptor  {} ({} bytes)",
        descriptor_path.display(),
        bytes.len()
    );
    println!("format      v{}", descriptor.version);
    println!(
        "root        {} (v{}, hash {})",
        if descriptor.root.name.is_empty() {
            "(none)"
        } else {
            &descriptor.root.name
        },
        descriptor.root.version,
        debug_hash(root_hash)
    );
    if !descriptor.root.scalars.is_empty() {
        println!("root fields ({})", descriptor.root.scalars.len());
        for field in &descriptor.root.scalars {
            print_debug_field(field, 2);
        }
    }
    println!("schemas ({})", descriptor.schemas.len());
    for schema in &descriptor.schemas {
        println!(
            "  {} (v{}, hash {}, {} fields)",
            schema.name,
            schema.version,
            debug_hash(Some(wire_hash(schema))),
            schema.fields.len()
        );
        for field in &schema.fields {
            print_debug_field(field, 4);
        }
    }
    println!("collections ({})", descriptor.collections.len());
    for (index, (name, schema_index)) in descriptor.collections.iter().enumerate() {
        let schema_name = descriptor
            .schemas
            .get(*schema_index)
            .map(|schema| schema.name.as_str())
            .unwrap_or("?");
        println!("  {index} {name} -> {schema_name}");
    }
}

fn debug_hash(hash: Option<u32>) -> String {
    hash.map(|value| format!("0x{value:08x}"))
        .unwrap_or_else(|| "n/a".into())
}

fn debug_flags(flags: u8) -> Vec<&'static str> {
    let mut names = Vec::new();
    if flags & FIELD_OPTIONAL != 0 {
        names.push("optional");
    }
    if flags & FIELD_STATIC != 0 {
        names.push("static");
    }
    if flags & FIELD_TRANSIENT != 0 {
        names.push("transient");
    }
    names
}

fn debug_type(field: &SchemaField) -> String {
    match field.kind {
        WireKind::Array => format!(
            "array<{}>",
            field.element.unwrap_or(WireKind::F64).builder_method()
        ),
        kind => kind.builder_method().to_string(),
    }
}

fn print_debug_field(field: &SchemaField, indent: usize) {
    let flags = debug_flags(field.flags);
    let suffix = if flags.is_empty() {
        String::new()
    } else {
        format!(" [{}]", flags.join(","))
    };
    println!(
        "{}{:<20} {}{suffix}",
        " ".repeat(indent),
        field.name,
        debug_type(field)
    );
}

fn print_schema_debug_json(
    path: &Path,
    byte_count: usize,
    descriptor: &engine_schema::Descriptor,
    root_hash: Option<u32>,
) {
    let fields: Vec<String> = descriptor
        .root
        .scalars
        .iter()
        .map(debug_field_json)
        .collect();
    let schemas: Vec<String> = descriptor
        .schemas
        .iter()
        .map(|schema| {
            let schema_fields: Vec<String> = schema.fields.iter().map(debug_field_json).collect();
            format!(
                "{{\"name\":\"{}\",\"version\":{},\"hash\":{},\"fields\":[{}]}}",
                json_escape(&schema.name),
                schema.version,
                wire_hash(schema),
                schema_fields.join(",")
            )
        })
        .collect();
    let collections: Vec<String> = descriptor
        .collections
        .iter()
        .enumerate()
        .map(|(index, (name, schema_index))| {
            let schema_name = descriptor
                .schemas
                .get(*schema_index)
                .map(|schema| schema.name.as_str())
                .unwrap_or("");
            format!(
                "{{\"index\":{index},\"name\":\"{}\",\"schema\":\"{}\"}}",
                json_escape(name),
                json_escape(schema_name)
            )
        })
        .collect();
    println!(
        "{{\"path\":\"{}\",\"bytes\":{},\"formatVersion\":{},\
         \"root\":{{\"name\":\"{}\",\"version\":{},\"hash\":{},\"fields\":[{}]}},\
         \"schemas\":[{}],\"collections\":[{}]}}",
        json_escape(&path.display().to_string()),
        byte_count,
        descriptor.version,
        json_escape(&descriptor.root.name),
        descriptor.root.version,
        root_hash
            .map(|hash| hash.to_string())
            .unwrap_or_else(|| "null".into()),
        fields.join(","),
        schemas.join(","),
        collections.join(","),
    );
}

fn debug_field_json(field: &SchemaField) -> String {
    let flags: Vec<String> = debug_flags(field.flags)
        .iter()
        .map(|flag| format!("\"{flag}\""))
        .collect();
    let element = field
        .element
        .map(|kind| format!("\"{}\"", kind.builder_method()))
        .unwrap_or_else(|| "null".into());
    format!(
        "{{\"name\":\"{}\",\"type\":\"{}\",\"code\":{},\"size\":{},\"flags\":[{}],\"element\":{}}}",
        json_escape(&field.name),
        field.kind.builder_method(),
        field.kind.code(),
        field.kind.size(),
        flags.join(","),
        element
    )
}

fn print_schema_output(
    output: &schema::SchemaOutput,
    dump: bool,
    json: bool,
) -> std::io::Result<()> {
    let descriptor = std::fs::read(&output.descriptor)?;
    let hex: String = descriptor
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();

    if dump {
        println!("{hex}");
        return Ok(());
    }

    if json {
        let wrote: Vec<String> = output
            .wrote
            .iter()
            .map(|path| format!("\"{}\"", json_escape(&path.display().to_string())))
            .collect();
        println!(
            "{{\"source\":\"{}\",\"server\":\"{}\",\"descriptor\":\"{}\",\
             \"descriptorBytes\":{},\"descriptorHex\":\"{}\",\
             \"clientDts\":\"{}\",\"wrote\":[{}]}}",
            json_escape(&output.source.display().to_string()),
            json_escape(&output.server.display().to_string()),
            json_escape(&output.descriptor.display().to_string()),
            descriptor.len(),
            hex,
            json_escape(&output.client_dts.display().to_string()),
            wrote.join(","),
        );
        return Ok(());
    }

    if output.wrote.is_empty() {
        println!("✓ schema outputs are up to date");
        return Ok(());
    }
    println!("✓ schema generated from {}:", output.source.display());
    println!("  • {} (AssemblyScript server)", output.server.display());
    println!("  • {} (browser descriptor)", output.descriptor.display());
    println!("  • {} (browser types)", output.client_dts.display());
    Ok(())
}

fn json_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}

/// `ox-game aot [dir]` — compile room source to WASM and AOT precompile to native `.cwasm`.
fn aot(dir: &Path) -> std::io::Result<()> {
    let cfg =
        if dir.join("config.toml").exists() || dir.join("config.yml").exists() || dir.is_file() {
            engine_config::load_config(dir)?
        } else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("config.toml not found in {}", dir.display()),
            ));
        };

    if dir.is_dir() {
        schema::generate_project(dir)?;
    }

    let wasm_path = engine_server::compile::build(dir, &cfg)?;
    let cwasm_path = engine_server::compile::aot_precompile(&wasm_path)?;
    println!("✓ Successfully AOT precompiled room to native machine code:");
    println!("  • WASM bytecode:       {}", wasm_path.display());
    println!("  • Native AOT (.cwasm): {}", cwasm_path.display());
    Ok(())
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
    std::fs::write(dir.join("schema.ts"), SCHEMA_TS_TEMPLATE)?;
    // The easy template is self-contained: server SDK, browser SDK and the
    // matching page all use OpenOxide's own protocol (no Colyseus dependency).
    std::fs::write(
        dir.join("openoxide.ts"),
        include_str!("../sdk/interfaces/assemblyscript/openoxide.ts"),
    )?;
    std::fs::write(
        dir.join("openoxide.d.ts"),
        include_str!("../sdk/interfaces/typescript/openoxide.d.ts"),
    )?;
    std::fs::write(
        dir.join("openoxide.js"),
        include_bytes!("../sdk/web/openoxide.js"),
    )?;
    std::fs::write(
        dir.join("openoxide-client.d.ts"),
        include_bytes!("../sdk/web/openoxide.d.ts"),
    )?;
    std::fs::write(
        dir.join("website.html"),
        include_bytes!("../crates/engine-config/src/templates/easy_website.html"),
    )?;
    let gitignore = dir.join(".gitignore");
    if !gitignore.exists() {
        std::fs::write(
            &gitignore,
            "# Regenerable schema client bindings (ox-game schema-codegen)\ngenerated/\n",
        )?;
    }

    // schema.ts is the single source for both runtime halves. Generating here
    // means the room template's generated import resolves immediately.
    let generated = schema::require_project(dir)?;

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
    println!(
        "  {}/schema.ts      — network state (Colyseus v5 syntax, edit me!)",
        dir.display()
    );
    println!(
        "  {}/openoxide.ts   — server SDK (Room, messages, SchemaState)",
        dir.display()
    );
    println!(
        "  {}/openoxide.js   — browser SDK (Client, schema, messages)",
        dir.display()
    );
    println!(
        "  {}/openoxide-client.d.ts — browser SDK types",
        dir.display()
    );
    println!(
        "  {}/.gitignore      — ignores regenerable client bindings",
        dir.display()
    );
    println!();
    print_schema_output(&generated, false, false)?;
    println!();
    println!("next: ox-game run {}", dir.display());
    Ok(())
}

/// `ox-game run [dir]` — compile (if needed) and serve.
fn run(dir: &Path) -> std::io::Result<()> {
    let cfg =
        if dir.join("config.toml").exists() || dir.join("config.yml").exists() || dir.is_file() {
            engine_config::load_config(dir)?
        } else if dir == Path::new(".")
            && !Path::new("config.toml").exists()
            && !Path::new("config.yml").exists()
        {
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

    if let Some(output) = schema::generate_project(&project_dir)? {
        if !output.wrote.is_empty() {
            eprintln!(
                "[schema] generated {} file(s) from {}",
                output.wrote.len(),
                output.source.display()
            );
        }
    }

    println!("🧩 room runtime: {:?}", cfg.runtime);
    // A project that ships only Unity C# gameplay has no room source at all.
    // That is not fatal — the server boots in Unity-relay mode. Only a genuine
    // build failure (source present but uncompilable) is reported.
    let src_path = project_dir.join(&cfg.source);
    let wasm_path = match engine_server::compile::build(&project_dir, &cfg) {
        Ok(path) => path,
        Err(_) if !src_path.exists() => {
            eprintln!(
                "[main] 🧩 no room source ({}) — running Unity-relay rooms only",
                cfg.source.display()
            );
            cfg.out.clone()
        }
        Err(e) => return Err(e),
    };

    // One bus for both planes: the sim thread owns the consumer ends, the
    // control plane only ever `try_send`s onto the producer ends, and the host
    // loop is a `status_tx` producer — it is the default game path, so for most
    // projects it is the only thing that ever publishes telemetry.
    let bus = engine_bus::Bus::new(4096, 8192);

    // Optional raw-UDP + pinned 128 Hz sim plane. Gated by OX_PLANES=1 so the
    // WS/WebRTC flow is untouched by default.
    let mut plane_cfg = engine_server::plane::PlaneConfig::from_env(&project_dir);
    plane_cfg.wasm_path = wasm_path.clone();
    if plane_cfg.enabled {
        let udp_addr = plane_cfg.udp_addr.clone();
        match engine_server::plane::spawn_sim_plane(bus.clone(), plane_cfg) {
            Ok(_) => eprintln!("[main] 🧵 sim plane spawned (OX_PLANES=1, udp {udp_addr})"),
            Err(e) => eprintln!("[main] ✖ sim plane spawn failed: {e}"),
        }
    }

    // The Tokio/Axum control plane: matchmake, seat tokens, WS signaling,
    // /status and statics. Bound before `host::run` starts so a
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

    let signals = _control.as_ref().map(|plane| plane.signals());
    engine_server::host::run(&project_dir, &wasm_path, &cfg, bus, signals)
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

    std::fs::write(
        &lua_path,
        include_str!("../sdk/interfaces/lua/openoxide.lua"),
    )?;
    std::fs::write(&c_path, include_str!("../sdk/interfaces/c/openoxide.h"))?;
    std::fs::write(
        &cpp_path,
        include_str!("../sdk/interfaces/cpp/openoxide.hpp"),
    )?;
    std::fs::write(
        &rust_path,
        include_str!("../sdk/interfaces/rust/openoxide_room.rs"),
    )?;
    std::fs::write(
        &csharp_path,
        include_str!("../sdk/interfaces/csharp/OpenOxide.cs"),
    )?;
    std::fs::write(
        &as_path,
        include_str!("../sdk/interfaces/assemblyscript/openoxide.ts"),
    )?;
    std::fs::write(
        &ts_path,
        include_str!("../sdk/interfaces/typescript/openoxide.d.ts"),
    )?;
    std::fs::write(&readme_path, include_str!("../sdk/interfaces/README.md"))?;

    println!(
        "✓ Successfully auto-generated OpenOxide Room Interfaces in {}:",
        dir.display()
    );
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
