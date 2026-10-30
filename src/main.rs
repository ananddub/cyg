//! # cyg-game — CYG game server CLI
//!
//!   cyg-game init \[dir]    create a game project (room.ts + schema.ts + own SDK + website)
//!   cyg-game run  \[dir]    compile (if needed) and run the server
//!   cyg-game run            (with no config.toml present) runs the built-in demo
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

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod backup;
mod cook;
mod schema;

fn init_logging() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_writer(std::io::stderr);
    if std::env::var("OX_LOG_FORMAT").is_ok_and(|v| v.eq_ignore_ascii_case("json")) {
        builder.json().init();
    } else {
        builder.compact().init();
    }
}

fn main() {
    init_logging();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("run");
    let dir = args
        .get(1)
        .filter(|a| !a.starts_with("--"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    match cmd {
        "init" => {
            if let Err(e) = init(&dir) {
                tracing::error!("✗ init failed: {e}");
                std::process::exit(1);
            }
        }
        "run" => {
            if let Err(e) = run(&dir) {
                tracing::error!("✗ run failed: {e}");
                std::process::exit(1);
            }
        }
        "aot" | "build" => {
            if let Err(e) = aot(&dir) {
                tracing::error!("✗ aot build failed: {e}");
                std::process::exit(1);
            }
        }
        "gen-interface" | "interfaces" | "sdk" => {
            if let Err(e) = generate_interfaces(&dir) {
                tracing::error!("✗ gen-interface failed: {e}");
                std::process::exit(1);
            }
        }
        "schema" | "gen-schema" => schema_command(&args[1..]),
        "doctor" => {
            let check_schema = args.iter().any(|a| a == "--check-schema");
            if let Err(e) = doctor(&dir, check_schema) {
                tracing::info!("✗ doctor found problems");
                std::process::exit(1);
            }
        }
        "schema-codegen" | "schema-gen" => schema_codegen_command(&args[1..]),
        "schema-debug" | "schema-inspect" => schema_debug_command(&args[1..]),
        "cook" => {
            if args.len() < 3 {
                tracing::info!("Usage: cyg-game cook <input.glb> <output.bin>");
                std::process::exit(1);
            }
            let input = Path::new(&args[1]);
            let output = Path::new(&args[2]);
            if let Err(e) = cook::cook_glb(input, output) {
                tracing::error!("✗ cook failed: {e}");
                std::process::exit(1);
            }
        }
        "backup" => backup_command(&dir, &args[1..]),
        "restore" => restore_command(&args[1..]),
        "--help" | "-h" | "help" => print_help(),
        other => {
            tracing::info!("unknown command: {other}");
            print_help();
            std::process::exit(1);
        }
    }
}

/// `cyg-game doctor [dir]` — one command that validates everything a project
/// needs: config, schema, wasm presence and the exact ABI export names the
/// host will look up, plus the secrets the boot guard would refuse to start
/// with. Catches contract mismatches (e.g. `tick()` vs
/// `on_tick()`) in seconds instead of at room-join time.
fn doctor(dir: &Path, check_schema: bool) -> std::io::Result<()> {
    let mut problems = 0usize;
    tracing::info!("🩺 cyg-game doctor — {}\n", dir.display());

    // 1. config
    let cfg = match engine_config::load_config(dir) {
        Ok(cfg) => {
            tracing::info!(
                "✓ config.toml parses (name '{}', tick {}Hz)",
                cfg.name,
                cfg.tick_rate
            );
            Some(cfg)
        }
        Err(e) => {
            tracing::info!("✗ config.toml: {e}");
            problems += 1;
            None
        }
    };

    // 2. schema
    match schema::load_contract(dir) {
        Ok(contract) => tracing::info!(
            "✓ schema.ts parses ({} schemas, root '{}')",
            contract.schemas.len(),
            contract.root.name
        ),
        Err(e) => {
            tracing::info!("✗ schema: {e}");
            problems += 1;
        }
    }

    // 3. room wasm + export names
    let required = [
        "on_create",
        "on_join",
        "on_leave",
        "on_alloc",
        "on_input",
        "on_tick",
        "on_save",
        "on_load",
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
                    let exports: Vec<String> =
                        module.exports().map(|e| e.name().to_string()).collect();
                    let missing: Vec<&str> = required
                        .iter()
                        .filter(|r| !exports.contains(&r.to_string()))
                        .copied()
                        .collect();
                    if missing.is_empty() {
                        tracing::info!(
                            "✓ room wasm exports the full host ABI ({} exports)",
                            exports.len()
                        );
                    } else {
                        tracing::info!(
                            "✗ room wasm is missing host exports: {}. The host looks up exactly these names (see sdk/server). Got: {}",
                            missing.join(", "),
                            exports.join(", ")
                        );
                        problems += 1;
                    }
                }
                Err(e) => {
                    tracing::info!("✗ room wasm does not compile: {e}");
                    problems += 1;
                }
            }
        }
        None => tracing::info!(
            "• room wasm not built yet (run: cyg-game run {})",
            dir.display()
        ),
    }

    // 4. secrets. The boot guard refuses to start a plane on these, so asking
    //    the same questions here names every problem before a deploy instead of
    //    leaving them to a process that will not come up.
    match cfg.as_ref() {
        None => tracing::info!("• secrets not audited (config.toml did not parse)"),
        Some(cfg) => {
            let read_env = |name: &str| std::env::var(name).ok();
            let checks = engine_config::audit_secrets(
                &cfg.control,
                engine_config::Rotation::global(),
                &read_env,
            );
            let failed: Vec<&str> = checks
                .iter()
                .filter(|c| !c.ok)
                .map(|c| c.detail.as_str())
                .collect();
            if failed.is_empty() {
                tracing::info!(
                    "✓ secrets are strong and no credential is accepted unverified ({} checks)",
                    checks.len()
                );
            } else {
                for detail in &failed {
                    tracing::info!("✗ {detail}");
                }
                tracing::info!("  rotation runbook: docs/SECRETS.md");
                problems += failed.len();
            }
        }
    }

    if check_schema {
        match cfg.as_ref().map(|c| c.control.session.clone()) {
            None => {
                tracing::info!("✗ --check-schema needs a parseable config.toml");
                problems += 1;
            }
            Some(session) if session.database_endpoint.is_empty() => {
                tracing::info!("✗ --check-schema: no database endpoint is configured");
                problems += 1;
            }
            Some(session) => match schema_status(&session) {
                Ok((applied, pending)) => {
                    let at = applied
                        .iter()
                        .max()
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "base".into());
                    tracing::info!(
                        "• schema at version {at}, {} pending step(s)",
                        pending.len()
                    );
                    for name in &pending {
                        tracing::info!("  pending: {name}");
                    }
                }
                Err(e) => {
                    tracing::info!("✗ --check-schema: {e}");
                    problems += 1;
                }
            },
        }
    }

    if problems > 0 {
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("{problems} problem(s)"),
        ))
    } else {
        tracing::info!("\n✓ project looks healthy");
        Ok(())
    }
}

fn block_on<T>(future: impl std::future::Future<Output = Result<T, String>>) -> Result<T, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    runtime.block_on(future)
}

fn backup_command(dir: &Path, args: &[String]) {
    if args.iter().any(|a| a == "--out")
        && args.iter().position(|a| a == "--out") == Some(args.len() - 1)
    {
        tracing::info!("Usage: cyg-game backup [dir] [--out backups/]");
        std::process::exit(1);
    }
    let out = args
        .iter()
        .position(|a| a == "--out")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("backups"));
    let cfg = match engine_config::load_config(dir) {
        Ok(cfg) => cfg,
        Err(e) => {
            tracing::error!("✗ backup: {e}");
            std::process::exit(1);
        }
    };
    match block_on(backup::backup(&cfg.control.session, &out)) {
        Ok(artifacts) => tracing::info!(
            "✓ backup written to {} (checksum {})",
            artifacts.archive.display(),
            artifacts.checksum.display()
        ),
        Err(e) => {
            tracing::error!("✗ backup: {e}");
            std::process::exit(1);
        }
    }
}

fn restore_command(args: &[String]) {
    let force = args.iter().any(|a| a == "--force");
    let positional: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    // `restore <archive>` and `restore <dir> <archive>` both mean "the last
    // path is the archive"; the leading one is only a project directory.
    let (dir, archive) = match positional.as_slice() {
        [dir, archive] => (PathBuf::from(dir), PathBuf::from(archive)),
        [archive] => (PathBuf::from("."), PathBuf::from(archive)),
        _ => {
            tracing::info!("Usage: cyg-game restore [dir] <archive.sql.gz> [--force]");
            std::process::exit(1);
        }
    };
    let dir = dir.as_path();
    let cfg = match engine_config::load_config(dir) {
        Ok(cfg) => cfg,
        Err(e) => {
            tracing::error!("✗ restore: {e}");
            std::process::exit(1);
        }
    };
    match block_on(backup::restore(&cfg.control.session, &archive, force)) {
        Ok(()) => tracing::info!("✓ restored {}", archive.display()),
        Err(e) => {
            tracing::error!("✗ restore: {e}");
            std::process::exit(1);
        }
    }
}

fn schema_status(
    session: &engine_config::SessionConfig,
) -> Result<(Vec<u64>, Vec<String>), String> {
    block_on(async move {
        let user = (!session.database_user.is_empty()).then_some(session.database_user.as_str());
        let pass = (!session.database_pass.is_empty()).then_some(session.database_pass.as_str());
        let db = engine_api::db::Datastore::open(
            &session.database_endpoint,
            &session.database_ns,
            &session.database_db,
            user,
            pass,
            engine_api::db::KvBudget::default(),
        )
        .await
        .map_err(|e| format!("{}: {e}", session.database_endpoint))?;
        let applied = engine_api::db::migrations::applied(&db)
            .await
            .map_err(|e| {
                format!("schema_version unreadable (boot the server once to create it): {e}")
            })?;
        let pending =
            engine_api::db::migrations::pending(&db, engine_api::db::migrations::MIGRATIONS)
                .await
                .map_err(|e| e.to_string())?;
        Ok((applied.into_iter().map(|a| a.version).collect(), pending))
    })
}

fn print_help() {
    tracing::info!(
        "cyg-game — CYG game server

USAGE:
  cyg-game init          [dir]    create a game project (room.ts + schema.ts + own SDK)
  cyg-game run           [dir]    compile the room (if needed) and start the server
  cyg-game aot           [dir]    AOT precompile room to native .cwasm for <1ms cold starts
  cyg-game gen-interface [dir]    auto-generate C++, Rust, C#, AssemblyScript & TypeScript interfaces
  cyg-game cook <in.glb> <out.bin> extract 3D collision geometry to instant-boot OXCL binary
  cyg-game schema [dir] --dump     compile schema.ts and print the descriptor as hex
  cyg-game schema [dir] --json     compile schema.ts and print a machine-readable report
  cyg-game schema        [dir]    compile schema.ts to server code + browser descriptor
  cyg-game schema-codegen [dir]   emit typed client bindings (--lang cs|gdscript|cpp|lua|all)
  cyg-game schema-debug [path]    inspect a compiled descriptor (--json)
  cyg-game doctor       [dir]    validate config, schema, wasm ABI exports and secrets
  cyg-game doctor --check-schema report pending SurrealDB schema migrations
  cyg-game backup  [dir] [--out backups]  export the HTTP database to a timestamped gzip + sha256 sidecar
  cyg-game restore [dir] <archive> [--force]  import a backup archive produced by backup
  cyg-game                        same as `run` in the current directory

CONFIG:
  Everything lives in config.toml — runtime, source file, ports, tick rate.
  See the template `cyg-game init` writes."
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
                tracing::info!(
                    "usage: cyg-game schema [dir] [--dump] [--json]\n\
                     \x20 --dump   print the binary descriptor as lowercase hex\n\
                     \x20 --json   print paths, byte count and descriptor hex as JSON"
                );
                return;
            }
            other if other.starts_with('-') => {
                tracing::info!("✗ unknown schema option: {other}");
                std::process::exit(1);
            }
            other if dir == Path::new(".") => dir = PathBuf::from(other),
            other => {
                tracing::info!("✗ unexpected schema argument: {other}");
                std::process::exit(1);
            }
        }
    }

    match schema::require_project(&dir) {
        Ok(output) => {
            if let Err(e) = print_schema_output(&output, dump, json) {
                tracing::error!("✗ schema report failed: {e}");
                std::process::exit(1);
            }
        }
        Err(e) => {
            tracing::error!("✗ schema generation failed: {e}");
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
                        tracing::info!("✗ --lang needs a value (cs, gdscript, cpp, lua or all)");
                        std::process::exit(1);
                    }
                }
            }
            "--out" => {
                index += 1;
                match args.get(index) {
                    Some(value) => out = Some(PathBuf::from(value)),
                    None => {
                        tracing::info!("✗ --out needs a directory");
                        std::process::exit(1);
                    }
                }
            }
            "--check" => check = true,
            "--help" | "-h" => {
                tracing::info!(
                    "usage: cyg-game schema-codegen [dir] [--lang <cs|gdscript|cpp|lua|all>] [--out DIR] [--check]\n\
                     \x20 --lang    target language; defaults to all\n\
                     \x20 --out     output directory; defaults to <dir>/generated\n\
                     \x20 --check   fail if generated files are missing or stale (CI)"
                );
                return;
            }
            other if other.starts_with('-') => {
                tracing::info!("✗ unknown schema-codegen option: {other}");
                std::process::exit(1);
            }
            other if dir == Path::new(".") => dir = PathBuf::from(other),
            other => {
                tracing::info!("✗ unexpected schema-codegen argument: {other}");
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
                tracing::info!("✗ unknown language '{lang}'; use cs, gdscript, cpp, lua or all");
                std::process::exit(1);
            }
        }
    };

    let contract = match schema::load_contract(&dir) {
        Ok(contract) => contract,
        Err(e) => {
            tracing::error!("✗ schema-codegen failed: {e}");
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
                tracing::info!(
                    "✓ {} up to date {}",
                    language.display_name(),
                    path.display()
                );
            }
        }
        if !stale.is_empty() {
            for path in stale {
                tracing::info!("✗ stale {}", path.display());
            }
            tracing::info!(
                "run `cyg-game schema-codegen {} --out {}` to regenerate",
                dir.display(),
                out_dir.display()
            );
            std::process::exit(1);
        }
        return;
    }

    for language in languages {
        match schema::write_codegen(&out_dir, &contract, language) {
            Ok((path, changed)) => tracing::info!(
                "✓ {} {} {}",
                language.display_name(),
                if changed { "wrote" } else { "unchanged" },
                path.display()
            ),
            Err(e) => {
                tracing::error!("✗ schema-codegen failed: {e}");
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
                tracing::info!(
                    "usage: cyg-game schema-debug [dir|file] [--json]\n\
                     \x20 Reads cyg.schema.bin (or the given file) and prints the\n\
                     \x20 root, collections, schemas, fields and recomputed hashes."
                );
                return;
            }
            other if other.starts_with('-') => {
                tracing::info!("✗ unknown schema-debug option: {other}");
                std::process::exit(1);
            }
            other if path == Path::new(".") => path = PathBuf::from(other),
            other => {
                tracing::info!("✗ unexpected schema-debug argument: {other}");
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
            tracing::info!("✗ cannot read {}: {e}", descriptor_path.display());
            std::process::exit(1);
        }
    };
    let Some(descriptor) = parse_descriptor(&bytes) else {
        tracing::info!(
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

    tracing::info!(
        "descriptor  {} ({} bytes)",
        descriptor_path.display(),
        bytes.len()
    );
    tracing::info!("format      v{}", descriptor.version);
    tracing::info!(
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
        tracing::info!("root fields ({})", descriptor.root.scalars.len());
        for field in &descriptor.root.scalars {
            print_debug_field(field, 2);
        }
    }
    tracing::info!("schemas ({})", descriptor.schemas.len());
    for schema in &descriptor.schemas {
        tracing::info!(
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
    tracing::info!("collections ({})", descriptor.collections.len());
    for (index, (name, schema_index)) in descriptor.collections.iter().enumerate() {
        let schema_name = descriptor
            .schemas
            .get(*schema_index)
            .map(|schema| schema.name.as_str())
            .unwrap_or("?");
        tracing::info!("  {index} {name} -> {schema_name}");
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
    tracing::info!(
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
    tracing::info!(
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
        tracing::info!("{hex}");
        return Ok(());
    }

    if json {
        let wrote: Vec<String> = output
            .wrote
            .iter()
            .map(|path| format!("\"{}\"", json_escape(&path.display().to_string())))
            .collect();
        tracing::info!(
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
        tracing::info!("✓ schema outputs are up to date");
        return Ok(());
    }
    tracing::info!("✓ schema generated from {}:", output.source.display());
    tracing::info!("  • {} (AssemblyScript server)", output.server.display());
    tracing::info!("  • {} (browser descriptor)", output.descriptor.display());
    tracing::info!("  • {} (browser types)", output.client_dts.display());
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

/// `cyg-game aot [dir]` — compile room source to WASM and AOT precompile to native `.cwasm`.
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
    tracing::info!("✓ Successfully AOT precompiled room to native machine code:");
    tracing::info!("  • WASM bytecode:       {}", wasm_path.display());
    tracing::info!("  • Native AOT (.cwasm): {}", cwasm_path.display());
    Ok(())
}

/// `cyg-game init [dir]` — scaffold a project.
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
    let src = dir.join("src");
    std::fs::create_dir_all(src.join("schema"))?;
    let pkg = dir.join("pkg");
    std::fs::create_dir_all(&pkg)?;
    std::fs::write(src.join("index.ts"), ROOM_TEMPLATE)?;
    std::fs::write(src.join("schema/player.ts"), SCHEMA_TS_TEMPLATE)?;
    std::fs::write(
        pkg.join("cyg-server.ts"),
        include_str!("../sdk/server/assemblyscript/cyg-server.ts"),
    )?;
    std::fs::write(
        pkg.join("cyg.d.ts"),
        include_str!("../sdk/server/typescript/cyg.d.ts"),
    )?;
    // The easy template is self-contained: server SDK, browser SDK and the
    // matching page all use CYG's own protocol (no Colyseus dependency).
    // VSCode/TS-server ko khush rakhta hai: pkg/ (AssemblyScript) exclude,
    // AS types ambient stub — warna poori SDK laal dikh jaati hai.
    std::fs::write(
        dir.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "esnext",
    "module": "esnext",
    "moduleResolution": "bundler",
    "experimentalDecorators": true,
    "noEmit": true,
    "skipLibCheck": true,
    "types": []
  },
  "include": ["src", "gen", "pkg/as-types.d.ts"]
}
"#,
    )?;
    std::fs::write(
        pkg.join("as-types.d.ts"),
        r#"// AssemblyScript types as ambient stubs — sirf editor ke liye.
declare type i8 = number;
declare type i16 = number;
declare type i32 = number;
declare type i64 = number;
declare type u8 = number;
declare type u16 = number;
declare type u32 = number;
declare type u64 = number;
declare type usize = number;
declare type f32 = number;
declare type f64 = number;
declare function unchecked<T>(x: T): T;
declare module "cyg" {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  export function schema(def: any, name: string): any;
  export const t: any;
}
"#,
    )?;
    let gitignore = dir.join(".gitignore");
    if !gitignore.exists() {
        std::fs::write(
            &gitignore,
            "# Regenerable schema bindings (cyg-game schema-codegen)\ngen/\n",
        )?;
    }

    // schema.ts is the single source for both runtime halves. Generating here
    // means the room template's generated import resolves immediately.
    let generated = schema::require_project(dir)?;

    tracing::info!("✓ game project created in {}", dir.display());
    tracing::info!("");
    tracing::info!(
        "  {}/config.toml     — runtime, ports, tick rate",
        dir.display()
    );
    tracing::info!(
        "  {}/room.ts        — your room logic (edit me!)",
        dir.display()
    );
    tracing::info!(
        "  {}/schema.ts      — network state (Colyseus v5 syntax, edit me!)",
        dir.display()
    );
    tracing::info!(
        "  {}/cyg-server.ts — server SDK (Room, messages, SchemaState)",
        dir.display()
    );
    tracing::info!(
        "  {}/cyg.js   — browser SDK (Client, schema, messages)",
        dir.display()
    );
    tracing::info!("  {}/cyg-client.d.ts — browser SDK types", dir.display());
    tracing::info!(
        "  {}/.gitignore      — ignores regenerable client bindings",
        dir.display()
    );
    tracing::info!("");
    print_schema_output(&generated, false, false)?;
    tracing::info!("");
    tracing::info!("next: cyg-game run {}", dir.display());
    Ok(())
}

/// `cyg-game run [dir]` — compile (if needed) and serve.
fn run(dir: &Path) -> std::io::Result<()> {
    let cfg = if dir.join("config.toml").exists()
        || dir.join("config.yml").exists()
        || dir.is_file()
    {
        engine_config::load_config(dir)?
    } else if dir == Path::new(".")
        && !Path::new("config.toml").exists()
        && !Path::new("config.yml").exists()
    {
        // No project here: run the built-in demo (headless room).
        tracing::info!("[run] no config.toml found — running the built-in demo (headless room)");
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
        let out_dir = std::env::temp_dir().join("cyg-game-demo");
        std::fs::create_dir_all(&out_dir)?;
        let out = out_dir.join("room_logic.wasm");
        std::fs::write(&out, wasm)?;
        (PathBuf::from("."), GameConfig { out: out, ..cfg })
    } else {
        (dir.to_path_buf(), cfg.clone())
    };

    if let Some(output) = schema::generate_project(&project_dir)? {
        if !output.wrote.is_empty() {
            tracing::info!(
                "[schema] generated {} file(s) from {}",
                output.wrote.len(),
                output.source.display()
            );
        }
    }

    tracing::info!("🧩 room runtime: {:?}", cfg.runtime);
    // A project that ships only Unity C# gameplay has no room source at all.
    // That is not fatal — the server boots in Unity-relay mode. Only a genuine
    // build failure (source present but uncompilable) is reported.
    let src_path = project_dir.join(&cfg.source);
    let wasm_path = match engine_server::compile::build(&project_dir, &cfg) {
        Ok(path) => path,
        Err(_) if !src_path.exists() => {
            tracing::info!(
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
            Ok(_) => tracing::info!("[main] 🧵 sim plane spawned (OX_PLANES=1, udp {udp_addr})"),
            Err(e) => tracing::error!("[main] ✖ sim plane spawn failed: {e}"),
        }
    }

    // The Tokio/Axum control plane: matchmake, seat tokens, WS signaling,
    // /status and statics. Bound before `host::run` starts so a
    // port clash is reported here rather than swallowing the game loop.
    // Held for the whole run — dropping it closes every HTTP and socket.
    let _control = if cfg.control.enabled {
        match engine_control::spawn(&project_dir, &cfg, &cfg.control, bus.clone()) {
            Ok(plane) => {
                tracing::info!(
                    "[main] 🏛️  control plane on http://{}:{} (matchmake, /ws/signal, /status, statics)",
                    cfg.control.bind_addr,
                    plane.port
                );
                Some(plane)
            }
            Err(e) => {
                tracing::error!(
                    "[main] ✖ control plane on port {} failed to start: {e}",
                    cfg.control.api_port
                );
                None
            }
        }
    } else {
        tracing::info!("[main] control plane disabled ([control] enabled = false)");
        None
    };

    let signals = _control.as_ref().map(|plane| plane.signals());
    let boot_maps = _control
        .as_ref()
        .map(|plane| plane.boot_maps())
        .unwrap_or_default();
    let boot_module = _control.as_ref().and_then(|plane| plane.boot_module());
    engine_server::host::run(
        &project_dir,
        &wasm_path,
        &cfg,
        bus,
        signals,
        boot_maps,
        boot_module,
    )
}

/// `cyg-game gen-interface [dir]` — auto-generate Lua, C, C++, Rust, C#, AssemblyScript & TypeScript room interfaces.
fn generate_interfaces(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;

    let lua_path = dir.join("cyg.lua");
    let c_path = dir.join("cyg.h");
    let cpp_path = dir.join("cyg.hpp");
    let rust_path = dir.join("cyg_room.rs");
    let csharp_path = dir.join("CYG.cs");
    let as_path = dir.join("cyg-server.ts");
    let ts_path = dir.join("cyg.d.ts");
    let readme_path = dir.join("README.md");

    std::fs::write(&lua_path, include_str!("../sdk/server/lua/cyg.lua"))?;
    std::fs::write(&c_path, include_str!("../sdk/server/c/cyg.h"))?;
    std::fs::write(&cpp_path, include_str!("../sdk/server/cpp/cyg.hpp"))?;
    std::fs::write(&rust_path, include_str!("../sdk/server/rust/cyg_room.rs"))?;
    std::fs::write(&csharp_path, include_str!("../sdk/server/csharp/CYG.cs"))?;
    std::fs::write(
        &as_path,
        include_str!("../sdk/server/assemblyscript/cyg-server.ts"),
    )?;
    std::fs::write(&ts_path, include_str!("../sdk/server/typescript/cyg.d.ts"))?;
    std::fs::write(&readme_path, include_str!("../sdk/server/README.md"))?;

    tracing::info!(
        "✓ Successfully auto-generated CYG Room Interfaces in {}:",
        dir.display()
    );
    tracing::info!("  • {} (Lua Room SDK & Class)", lua_path.display());
    tracing::info!("  • {} (Pure C Procedural Header)", c_path.display());
    tracing::info!("  • {} (C++ Class & Macro Header)", cpp_path.display());
    tracing::info!("  • {} (Rust Guest Trait)", rust_path.display());
    tracing::info!("  • {} (C# .NET 8 Room Class)", csharp_path.display());
    tracing::info!("  • {} (AssemblyScript Room Base Class)", as_path.display());
    tracing::info!("  • {} (TypeScript Lifecycle Contract)", ts_path.display());
    tracing::info!("  • {} (Quick Start Guide)", readme_path.display());
    Ok(())
}
