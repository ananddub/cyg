//! `schema.ts` → server state-sync code + binary descriptor, shared by
//! init/run/aot and the explicit `ox-game schema` command.
//!
//! The browser needs no schema file: the descriptor is delivered in the join
//! handshake, and `openoxide.schema.d.ts` provides the typed contract.

use std::io;
use std::path::{Path, PathBuf};

use engine_schema::codegen;
use engine_schema::ts;
use engine_schema::{RootState, SchemaDef};

pub const TS_SOURCE_FILE: &str = "schema.ts";
pub const SERVER_FILE: &str = "openoxide.schema.ts";
pub const CLIENT_DTS_FILE: &str = "openoxide.schema.d.ts";
pub const DESCRIPTOR_FILE: &str = "openoxide.schema.bin";

#[derive(Debug)]
pub struct SchemaOutput {
    /// The source that was compiled.
    pub source: PathBuf,
    pub server: PathBuf,
    pub client_dts: PathBuf,
    pub descriptor: PathBuf,
    pub wrote: Vec<PathBuf>,
}

fn find_source(dir: &Path) -> Option<PathBuf> {
    // Root first, then the conventional src/ layout.
    let root = dir.join(TS_SOURCE_FILE);
    if root.exists() {
        return Some(root);
    }
    let nested = dir.join("src").join(TS_SOURCE_FILE);
    nested.exists().then_some(nested)
}

/// Regenerate when `schema.ts` exists; do nothing for projects that have not
/// adopted the DSL schema.
pub fn generate_project(dir: &Path) -> io::Result<Option<SchemaOutput>> {
    match find_source(dir) {
        Some(source) => generate_file(dir, &source).map(Some),
        None => Ok(None),
    }
}

/// Regenerate and fail loudly when there is no `schema.ts`.
pub fn require_project(dir: &Path) -> io::Result<SchemaOutput> {
    let source = find_source(dir).ok_or_else(|| missing_source(dir))?;
    generate_file(dir, &source)
}

/// The parsed contract, without generating server-side files. Used by
/// `schema-codegen`, which only needs the field lists and the root world.
pub struct SchemaContract {
    pub source: PathBuf,
    pub schemas: Vec<SchemaDef>,
    pub root: RootState,
}

impl SchemaContract {
    pub fn source_name(&self) -> String {
        self.source
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.source.display().to_string())
    }
}

/// Read and validate the project schema without writing anything.
pub fn load_contract(dir: &Path) -> io::Result<SchemaContract> {
    let source = find_source(dir).ok_or_else(|| missing_source(dir))?;
    let text = std::fs::read_to_string(&source)?;
    let parsed = ts::parse(&text).map_err(|d| diagnostics_error(&source, d))?;
    let root = parsed.root.ok_or_else(|| missing_root(&source))?;
    Ok(SchemaContract {
        source,
        schemas: parsed.schemas,
        root,
    })
}

/// Render one client binding file without touching the filesystem.
pub fn codegen_contents(contract: &SchemaContract, language: codegen::Language) -> String {
    codegen::emit(
        language,
        &contract.schemas,
        Some(&contract.root),
        &contract.source_name(),
    )
}

/// Emit one client binding file and return its path plus whether it changed.
pub fn write_codegen(
    out_dir: &Path,
    contract: &SchemaContract,
    language: codegen::Language,
) -> io::Result<(PathBuf, bool)> {
    let mut contents = codegen_contents(contract, language);
    // AS output imports the SDK relatively; with the conventional src/ layout
    // the SDK lives in src/ beside room.ts — rewrite so the SDK is loaded once
    // (two copies in one compile = duplicate-class conflicts).
    if language == codegen::Language::As && dir_has_src(out_dir) {
        contents = contents.replace(
            "from \"../openoxide\"",
            "from \"../src/openoxide\"",
        );
    }
    std::fs::create_dir_all(out_dir)?;
    let path = out_dir.join(language.file_name());
    let changed = write_bytes_if_changed(&path, contents.as_bytes())?;
    Ok((path, changed))
}

fn dir_has_src(out_dir: &Path) -> bool {
    // out_dir is <project>/generated -> project root is its parent.
    out_dir
        .parent()
        .map(|p| p.join("src").exists())
        .unwrap_or(false)
}

fn missing_source(dir: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!(
            "no schema source found in {}; add `schema.ts` with\n  \
             const Player = schema({{ x: t.number(), y: t.number() }}, \"Player\");\n  \
             export const State = schema({{ players: t.map(Player) }}, \"State\");",
            dir.display()
        ),
    )
}

fn missing_root(source_path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!(
            "{}: schema.ts must export a `State` schema with at least one \
             `t.map(Entity)` collection, e.g.\n  \
             export const State = schema({{ players: t.map(Player) }}, \"State\");",
            source_path.display()
        ),
    )
}

fn generate_file(dir: &Path, source_path: &Path) -> io::Result<SchemaOutput> {
    let source = std::fs::read_to_string(source_path)?;
    let parsed = ts::parse(&source).map_err(|d| diagnostics_error(source_path, d))?;
    let root = parsed.root.ok_or_else(|| missing_root(source_path))?;
    let generated = engine_schema::generate_with_root(&parsed.schemas, Some(&root), TS_SOURCE_FILE);

    let server = dir.join(SERVER_FILE);
    let descriptor = dir.join(DESCRIPTOR_FILE);
    let mut wrote = Vec::new();
    if write_bytes_if_changed(&server, generated.server_ts.as_bytes())? {
        wrote.push(server.clone());
    }
    if write_bytes_if_changed(&descriptor, &generated.descriptor)? {
        wrote.push(descriptor.clone());
    }

    let client_dts = dir.join(CLIENT_DTS_FILE);
    if write_bytes_if_changed(&client_dts, generated.client_dts.as_bytes())? {
        wrote.push(client_dts.clone());
    }

    // AS output imports the SDK relatively; with the conventional src/ layout
    // the SDK lives in src/ beside room.ts — rewrite so the SDK is loaded once.
    if server.exists() {
        let fixed = std::fs::read_to_string(&server)
            .map(|t| t.replace("from \"../openoxide\"", "from \"../src/openoxide\""))
            .unwrap_or_default();
        if fixed.contains("from \"../src/openoxide\"") {
            let _ = std::fs::write(&server, fixed);
        }
    }

    Ok(SchemaOutput {
        source: source_path.to_path_buf(),
        server,
        client_dts,
        descriptor,
        wrote,
    })
}

fn diagnostics_error(
    source_path: &Path,
    diagnostics: Vec<engine_schema::SchemaDiagnostic>,
) -> io::Error {
    let mut message = String::new();
    for diagnostic in diagnostics {
        message.push_str(&format!(
            "{}:{}:{}: {}\n",
            source_path.display(),
            diagnostic.line,
            diagnostic.column,
            diagnostic.message
        ));
    }
    io::Error::new(io::ErrorKind::InvalidData, message.trim_end().to_string())
}

fn write_bytes_if_changed(path: &Path, contents: &[u8]) -> io::Result<bool> {
    if let Ok(existing) = std::fs::read(path) {
        if existing == contents {
            return Ok(false);
        }
    }
    std::fs::write(path, contents)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_project(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("ox-schema-{label}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn cyg_only_project_is_rejected() {
        let dir = temp_project("cyg");
        std::fs::write(dir.join("schema.cyg"), "schema Player : 1 { FP x; }").unwrap();
        let error = require_project(&dir).unwrap_err();
        assert!(
            error.to_string().contains("no schema source found"),
            "unexpected error: {error}"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn schema_without_state_is_rejected() {
        let dir = temp_project("no-root");
        std::fs::write(
            dir.join(TS_SOURCE_FILE),
            r#"export const Player = schema({ x: t.number() }, "Player");"#,
        )
        .unwrap();
        let error = require_project(&dir).unwrap_err();
        assert!(
            error.to_string().contains("must export a `State` schema"),
            "unexpected error: {error}"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
