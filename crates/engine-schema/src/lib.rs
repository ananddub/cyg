//! Parses Colyseus-v5-style `schema({...}, "Name")` calls from `schema.ts` and
//! generates the CYG state-sync contract:
//!
//! * `cyg.schema.ts` — AssemblyScript `SchemaDef` + `SchemaEntity` subclass.
//! * `cyg.schema.bin` — compact descriptor sent to clients on join.
//! * `cyg.schema.d.ts` — typed browser contract.
//!
//! Every output derives its field list from the same source, so the FNV-1a wire
//! hash is identical by construction. Field values are numeric/bool primitives,
//! fixed-width UTF-8 strings (255 bytes), or flat arrays of those primitives.
//! Vectors and asset refs have no schema encoding and are rejected with a
//! source location.

use std::collections::HashMap;
use std::fmt;

pub mod codegen;
pub mod ts;

/// The runtime's `schemaFieldSize` table. Keep in sync with
/// `sdk/server/assemblyscript/cyg.ts` and `sdk/client/web/cyg.js`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireKind {
    U8,
    I8,
    U16,
    I16,
    U32,
    I32,
    F32,
    F64,
    Bool,
    I64,
    /// Fixed-width UTF-8: `[u8 byte_len][bytes]`, at most 255 bytes.
    String,
    /// Flat `[u8 count]` followed by `count` values of `SchemaField::element`.
    Array,
}

/// `.optional()` — one presence byte precedes the value on the wire.
pub const FIELD_OPTIONAL: u8 = 1;
/// `.static()` — sent in full snapshots, never patched.
pub const FIELD_STATIC: u8 = 2;
/// `.transient()` — sent in full snapshots, never automatically patched.
pub const FIELD_TRANSIENT: u8 = 4;

impl WireKind {
    /// Fixed byte size in the entity row buffer. Variable-length kinds store
    /// their value in side storage and therefore reserve no row bytes.
    pub fn size(self) -> usize {
        match self {
            WireKind::U8 | WireKind::I8 | WireKind::Bool => 1,
            WireKind::U16 | WireKind::I16 => 2,
            WireKind::U32 | WireKind::I32 | WireKind::F32 => 4,
            WireKind::F64 | WireKind::I64 => 8,
            WireKind::String | WireKind::Array => 0,
        }
    }

    pub fn is_variable(self) -> bool {
        matches!(self, WireKind::String | WireKind::Array)
    }

    /// Inverse of [`Self::code`]. Unknown codes yield `None`.
    pub fn from_code(code: u8) -> Option<WireKind> {
        Some(match code {
            1 => WireKind::U8,
            2 => WireKind::U16,
            3 => WireKind::U32,
            4 => WireKind::I32,
            5 => WireKind::F32,
            6 => WireKind::F64,
            7 => WireKind::Bool,
            8 => WireKind::I64,
            9 => WireKind::I8,
            10 => WireKind::I16,
            11 => WireKind::String,
            12 => WireKind::Array,
            _ => return None,
        })
    }

    /// Wire type code. Must match `FieldType` in the AssemblyScript runtime and
    /// `SCHEMA_TYPE_CODE` in the browser SDK; the two feed the same FNV hash.
    pub fn code(self) -> u8 {
        match self {
            WireKind::U8 => 1,
            WireKind::U16 => 2,
            WireKind::U32 => 3,
            WireKind::I32 => 4,
            WireKind::F32 => 5,
            WireKind::F64 => 6,
            WireKind::Bool => 7,
            WireKind::I64 => 8,
            WireKind::I8 => 9,
            WireKind::I16 => 10,
            WireKind::String => 11,
            WireKind::Array => 12,
        }
    }

    /// Method name on both `SchemaDef` implementations (`u8`, `f64`, ...).
    pub fn builder_method(self) -> &'static str {
        match self {
            WireKind::U8 => "u8",
            WireKind::I8 => "i8",
            WireKind::U16 => "u16",
            WireKind::I16 => "i16",
            WireKind::U32 => "u32",
            WireKind::I32 => "i32",
            WireKind::F32 => "f32",
            WireKind::F64 => "f64",
            WireKind::Bool => "bool",
            WireKind::I64 => "i64",
            WireKind::String => "string",
            WireKind::Array => "array",
        }
    }

    /// Type inside the generated AssemblyScript class.
    pub fn assemblyscript_type(self) -> &'static str {
        match self {
            WireKind::U8 => "u8",
            WireKind::I8 => "i8",
            WireKind::U16 => "u16",
            WireKind::I16 => "i16",
            WireKind::U32 => "u32",
            WireKind::I32 => "i32",
            WireKind::F32 => "f32",
            WireKind::F64 => "f64",
            WireKind::Bool => "bool",
            WireKind::I64 => "i64",
            WireKind::String => "string",
            // Element-typed: callers use `array_assemblyscript_type(element)`.
            WireKind::Array => "Array<f64>",
        }
    }

    /// Type inside the generated browser `.d.ts` interface.
    pub fn typescript_type(self) -> &'static str {
        match self {
            WireKind::Bool => "boolean",
            WireKind::String => "string",
            WireKind::Array => "Array<number>",
            _ => "number",
        }
    }

    fn value_suffix(self) -> &'static str {
        match self {
            WireKind::U8 => "U8",
            WireKind::I8 => "I8",
            WireKind::U16 => "U16",
            WireKind::I16 => "I16",
            WireKind::U32 => "U32",
            WireKind::I32 => "I32",
            WireKind::F32 => "F32",
            WireKind::F64 => "F64",
            WireKind::Bool => "Bool",
            WireKind::I64 => "I64",
            WireKind::String => "String",
            WireKind::Array => "Array",
        }
    }
}

/// Flat arrays are exposed as numeric vectors on both sides; boolean elements
/// travel one byte each and read back as `0` / `1`.
pub fn array_assemblyscript_type(_element: WireKind) -> &'static str {
    "Array<f64>"
}

pub fn array_typescript_type(_element: WireKind) -> &'static str {
    "Array<number>"
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaField {
    pub name: String,
    pub kind: WireKind,
    /// Element type for `WireKind::Array`, `None` otherwise.
    pub element: Option<WireKind>,
    /// Target schema name for `t.ref(Target)`; the wire value is the entity id.
    pub ref_target: Option<String>,
    /// `FIELD_OPTIONAL | FIELD_STATIC | FIELD_TRANSIENT`.
    pub flags: u8,
    /// The type as written (`FP`, `WeaponType`, ...), kept for diagnostics.
    pub source_type: String,
    /// Optional constructor default from `.default(value)`. Stored as the
    /// normalized source literal so integer and boolean ranges can be checked
    /// without changing `SchemaField`'s `Eq` contract.
    pub default: Option<String>,
    pub line: usize,
    pub column: usize,
}

impl SchemaField {
    pub fn optional(&self) -> bool {
        self.flags & FIELD_OPTIONAL != 0
    }

    pub fn full_only(&self) -> bool {
        self.flags & (FIELD_STATIC | FIELD_TRANSIENT) != 0
    }
}

/// One root collection a `State` schema exposes to the browser
/// (`room.state.players`). Collections are synced independently and carry
/// their declaration index on the wire.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RootCollection {
    pub name: String,
    pub schema: String,
}

/// The root world declared by `schema.ts`'s `State` call: scalar fields live
/// on `room.state` itself, map fields become independent `SchemaMap`s.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RootState {
    pub name: String,
    pub version: u32,
    pub scalars: Vec<SchemaField>,
    pub collections: Vec<RootCollection>,
}

/// Descriptor `[u16 collection_count]`; keep the generated world small enough
/// that a single full snapshot stays well below the outbound delta limit.
pub const MAX_COLLECTIONS: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaDef {
    pub name: String,
    pub version: u32,
    pub fields: Vec<SchemaField>,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaDiagnostic {
    pub line: usize,
    pub column: usize,
    pub message: String,
}

impl fmt::Display for SchemaDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.column, self.message)
    }
}

#[derive(Debug)]
pub struct GeneratedSchema {
    pub server_ts: String,
    pub client_dts: String,
    /// Compact `cyg.schema.bin` payload sent by the host on join.
    pub descriptor: Vec<u8>,
}

/// Generate the server and browser halves for an already validated schema list
/// and its explicit root world.
pub fn generate_with_root(
    schemas: &[SchemaDef],
    root: Option<&RootState>,
    source_name: &str,
) -> GeneratedSchema {
    let source_name = source_name.replace(['\r', '\n'], " ");
    GeneratedSchema {
        server_ts: emit_server(schemas, root, &source_name),
        client_dts: emit_client_dts(schemas, root, &source_name),
        descriptor: emit_descriptor(schemas, root),
    }
}

/// FNV-1a over `name|version|field:code[.element][:flags]...`, bit-identical
/// to `SchemaDef` in the AssemblyScript and browser runtimes.
///
/// Element and flag suffixes are only emitted when present, so every schema
/// that used the pre-Phase-3 field set keeps its historical hash.
pub fn wire_hash(schema: &SchemaDef) -> u32 {
    fn mix(mut h: u32, text: &str) -> u32 {
        for byte in text.as_bytes() {
            h ^= *byte as u32;
            h = h.wrapping_mul(16_777_619);
        }
        h
    }

    let mut h = mix(2_166_136_261, &schema.name);
    h = mix(h, "|");
    h = mix(h, &schema.version.to_string());
    for field in &schema.fields {
        h = mix(h, "|");
        h = mix(h, &field.name);
        h = mix(h, ":");
        h = mix(h, &field.kind.code().to_string());
        if let Some(element) = field.element {
            h = mix(h, ".");
            h = mix(h, &element.code().to_string());
        }
        if field.flags != 0 {
            h = mix(h, ":");
            h = mix(h, &field.flags.to_string());
        }
    }
    h
}

pub const DESCRIPTOR_MAGIC: &[u8; 4] = b"OXSD";
/// v2 adds root scalars and multiple root collections; v3 adds per-field
/// flags (optional/static/transient) and array element types. Clients accept
/// v1, v2 and v3.
pub const DESCRIPTOR_VERSION: u8 = 3;

fn write_u16(out: &mut Vec<u8>, value: usize) {
    out.extend_from_slice(&(value as u16).to_le_bytes());
}

fn write_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_str(out: &mut Vec<u8>, value: &str) {
    write_u16(out, value.len());
    out.extend_from_slice(value.as_bytes());
}

fn write_field(out: &mut Vec<u8>, field: &SchemaField) {
    write_str(out, &field.name);
    out.push(field.kind.code());
    out.push(field.flags);
    out.push(field.element.map(WireKind::code).unwrap_or(0));
}

/// Binary schema descriptor consumed by `cyg.js`.
///
/// ```text
/// "OXSD" [u8 version=3] [u8 flags]
/// [u16 schema_count]
///   [str name] [u32 schema_version] [u32 hash] [u16 field_count]
///     [str field_name] [u8 wire_code] [u8 field_flags] [u8 element_code]
/// [str root_name] [u32 root_version] [u32 root_hash] [u16 root_field_count]
///     [str field_name] [u8 wire_code] [u8 field_flags] [u8 element_code]
/// [u16 collection_count]
///   [str collection_name] [u16 schema_index or 0xFFFF]
/// ```
pub fn emit_descriptor(schemas: &[SchemaDef], root: Option<&RootState>) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(DESCRIPTOR_MAGIC);
    out.push(DESCRIPTOR_VERSION);
    out.push(0);
    write_u16(&mut out, schemas.len());
    for schema in schemas {
        write_str(&mut out, &schema.name);
        write_u32(&mut out, schema.version);
        write_u32(&mut out, wire_hash(schema));
        write_u16(&mut out, schema.fields.len());
        for field in &schema.fields {
            write_field(&mut out, field);
        }
    }

    let empty = RootState {
        name: String::new(),
        version: 0,
        scalars: Vec::new(),
        collections: Vec::new(),
    };
    let root = root.unwrap_or(&empty);
    let root_schema = SchemaDef {
        name: root.name.clone(),
        version: root.version,
        fields: root.scalars.clone(),
        line: 0,
        column: 0,
    };
    write_str(&mut out, &root.name);
    write_u32(&mut out, root.version);
    write_u32(&mut out, wire_hash(&root_schema));
    write_u16(&mut out, root.scalars.len());
    for field in &root.scalars {
        write_field(&mut out, field);
    }

    write_u16(&mut out, root.collections.len());
    for collection in &root.collections {
        write_str(&mut out, &collection.name);
        let index = schemas
            .iter()
            .position(|schema| schema.name == collection.schema)
            .unwrap_or(u16::MAX as usize);
        write_u16(&mut out, index);
    }
    out
}

/// A decoded `cyg.schema.bin` descriptor (v1–v3).
///
/// The host keeps one of these next to the wire state so it can walk entity
/// rows (per-field sizes, optional presence bytes) without asking the guest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Descriptor {
    pub version: u8,
    pub schemas: Vec<SchemaDef>,
    pub root: RootState,
    /// Root collection name -> index into [`Descriptor::schemas`], in wire
    /// collection-id order.
    pub collections: Vec<(String, usize)>,
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Option<&'a [u8]> {
        let end = self.offset.checked_add(len)?;
        if end > self.bytes.len() {
            return None;
        }
        let slice = &self.bytes[self.offset..end];
        self.offset = end;
        Some(slice)
    }

    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }

    fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Option<u32> {
        self.take(4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn str(&mut self) -> Option<String> {
        let len = self.u16()? as usize;
        let bytes = self.take(len)?;
        String::from_utf8(bytes.to_vec()).ok()
    }
}

fn read_descriptor_field(r: &mut Reader<'_>, version: u8) -> Option<SchemaField> {
    let name = r.str()?;
    let kind = WireKind::from_code(r.u8()?)?;
    let mut flags = 0;
    let mut element = None;
    if version >= 3 {
        flags = r.u8()?;
        let element_code = r.u8()?;
        if element_code != 0 {
            element = Some(WireKind::from_code(element_code)?);
        }
    }
    Some(SchemaField {
        name,
        kind,
        element,
        ref_target: None,
        flags,
        source_type: kind.typescript_type().to_string(),
        default: None,
        line: 0,
        column: 0,
    })
}

/// Decode an `OXSD` descriptor. Returns `None` for a truncated, foreign or
/// newer-than-known payload; the caller treats that as "no host-side schema".
pub fn parse_descriptor(bytes: &[u8]) -> Option<Descriptor> {
    if bytes.len() < 6 || &bytes[0..4] != DESCRIPTOR_MAGIC {
        return None;
    }
    let mut r = Reader { bytes, offset: 4 };
    let version = r.u8()?;
    if version < 1 || version > DESCRIPTOR_VERSION {
        return None;
    }
    let _flags = r.u8()?;
    let schema_count = r.u16()? as usize;
    let mut schemas = Vec::with_capacity(schema_count);
    for _ in 0..schema_count {
        let name = r.str()?;
        let schema_version = r.u32()?;
        let _hash = r.u32()?;
        let field_count = r.u16()? as usize;
        let mut fields = Vec::with_capacity(field_count);
        for _ in 0..field_count {
            fields.push(read_descriptor_field(&mut r, version)?);
        }
        schemas.push(SchemaDef {
            name,
            version: schema_version,
            fields,
            line: 0,
            column: 0,
        });
    }

    let root_name = r.str()?;
    let mut collections: Vec<(String, usize)> = Vec::new();
    let root = if version == 1 {
        let index = r.u16()? as usize;
        schemas.get(index)?;
        if !root_name.is_empty() {
            collections.push((root_name.clone(), index));
        }
        RootState {
            name: root_name,
            version: 0,
            scalars: Vec::new(),
            collections: collections
                .iter()
                .map(|(name, index)| RootCollection {
                    name: name.clone(),
                    schema: schemas[*index].name.clone(),
                })
                .collect(),
        }
    } else {
        let root_version = r.u32()?;
        let _root_hash = r.u32()?;
        let scalar_count = r.u16()? as usize;
        let mut scalars = Vec::with_capacity(scalar_count);
        for _ in 0..scalar_count {
            scalars.push(read_descriptor_field(&mut r, version)?);
        }
        let collection_count = r.u16()? as usize;
        for _ in 0..collection_count {
            let name = r.str()?;
            let index = r.u16()? as usize;
            schemas.get(index)?;
            collections.push((name, index));
        }
        RootState {
            name: root_name,
            version: root_version,
            scalars,
            collections: collections
                .iter()
                .map(|(name, index)| RootCollection {
                    name: name.clone(),
                    schema: schemas[*index].name.clone(),
                })
                .collect(),
        }
    };
    if r.offset != bytes.len() {
        return None;
    }
    Some(Descriptor {
        version,
        schemas,
        root,
        collections,
    })
}
const MAX_FIELDS: usize = 32;

fn check_schema_name(
    name: &str,
    line: usize,
    column: usize,
    diagnostics: &mut Vec<SchemaDiagnostic>,
) {
    if is_reserved_word(name) {
        diagnostics.push(SchemaDiagnostic {
            line,
            column,
            message: format!("schema name '{name}' is a reserved word"),
        });
    }
}

fn check_unique_names(schemas: &[SchemaDef], diagnostics: &mut Vec<SchemaDiagnostic>) {
    let mut seen: HashMap<&str, (usize, usize)> = HashMap::new();
    for schema in schemas {
        if let Some((line, column)) = seen.insert(&schema.name, (schema.line, schema.column)) {
            diagnostics.push(SchemaDiagnostic {
                line: schema.line,
                column: schema.column,
                message: format!(
                    "schema '{name}' is declared more than once (first at {line}:{column})",
                    name = schema.name
                ),
            });
        }
    }
}

fn is_reserved_field_name(name: &str) -> bool {
    matches!(
        name,
        "id" | "schemaHash" | "dirtyMask" | "clearDirty" | "schema"
    ) || is_reserved_word(name)
}

fn is_reserved_word(name: &str) -> bool {
    matches!(
        name,
        "as" | "break"
            | "case"
            | "catch"
            | "class"
            | "const"
            | "constructor"
            | "continue"
            | "debugger"
            | "declare"
            | "default"
            | "delete"
            | "do"
            | "else"
            | "enum"
            | "export"
            | "extends"
            | "false"
            | "finally"
            | "for"
            | "from"
            | "function"
            | "get"
            | "if"
            | "implements"
            | "import"
            | "in"
            | "instanceof"
            | "interface"
            | "let"
            | "namespace"
            | "new"
            | "null"
            | "of"
            | "package"
            | "private"
            | "protected"
            | "prototype"
            | "public"
            | "return"
            | "set"
            | "static"
            | "super"
            | "switch"
            | "this"
            | "throw"
            | "true"
            | "try"
            | "typeof"
            | "var"
            | "void"
            | "while"
            | "with"
            | "yield"
            | "bool"
            | "f32"
            | "f64"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "string"
            | "SchemaDef"
            | "SchemaEntity"
            | "SchemaState"
            | "CYG"
    )
}

// ---------------------------------------------------------------------------
// Emit
// ---------------------------------------------------------------------------

const HEADER: &str = "// <auto-generated />";

fn capitalize(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn assemblyscript_string_literal(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            control if control.is_control() => {
                out.push_str(&format!("\\u{{{:x}}}", control as u32));
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// Render a validated `.default()` literal as AssemblyScript source.
fn default_literal(kind: WireKind, literal: &str) -> String {
    match kind {
        WireKind::Bool => literal.to_string(),
        WireKind::String => assemblyscript_string_literal(literal),
        WireKind::F32 | WireKind::F64 => {
            if literal.contains('.') || literal.contains('e') || literal.contains('E') {
                literal.to_string()
            } else {
                format!("{literal}.0")
            }
        }
        _ => literal.to_string(),
    }
}

/// One `SchemaDef` builder line plus its flag modifiers.
fn emit_field_builder(out: &mut String, field: &SchemaField) {
    if field.kind == WireKind::Array {
        out.push_str(&format!(
            "    .array(\"{}\", {})\n",
            field.name,
            field.element.unwrap_or(WireKind::F64).code()
        ));
    } else {
        out.push_str(&format!(
            "    .{}(\"{}\")\n",
            field.kind.builder_method(),
            field.name
        ));
    }
    if field.flags & FIELD_OPTIONAL != 0 {
        out.push_str("    .optional()\n");
    }
    if field.flags & FIELD_STATIC != 0 {
        out.push_str("    .staticField()\n");
    }
    if field.flags & FIELD_TRANSIENT != 0 {
        out.push_str("    .transient()\n");
    }
}

fn emit_accessors(out: &mut String, index_prefix: &str, field: &SchemaField) {
    let ty = match field.kind {
        WireKind::Array => array_assemblyscript_type(field.element.unwrap_or(WireKind::F64)),
        other => other.assemblyscript_type(),
    };
    let suffix = field.kind.value_suffix();
    out.push_str(&format!(
        "\n    get {name}(): {ty} {{\n        return this.get{suffix}({prefix}_{name});\n    }}\n",
        name = field.name,
        prefix = index_prefix
    ));
    out.push_str(&format!(
        "    set {name}(v: {ty}) {{\n        this.set{suffix}({prefix}_{name}, v);\n    }}\n",
        name = field.name,
        prefix = index_prefix
    ));
    if field.optional() {
        let cap = capitalize(&field.name);
        out.push_str(&format!(
            "\n    has{cap}(): bool {{\n        return this.hasField({prefix}_{name});\n    }}\n",
            name = field.name,
            prefix = index_prefix
        ));
        out.push_str(&format!(
            "    clear{cap}(): void {{\n        this.clearField({prefix}_{name});\n    }}\n",
            prefix = index_prefix,
            name = field.name
        ));
    }
}

fn emit_server(schemas: &[SchemaDef], root: Option<&RootState>, source_name: &str) -> String {
    let mut out = String::new();
    out.push_str(HEADER);
    out.push('\n');
    out.push_str(&format!(
        "// Generated from {source_name} by `ox-game schema`. Edit the schema source, not this file.\n\n"
    ));
    if is_multi_root(root) {
        out.push_str(
            "import { SchemaDef, SchemaEntity, SchemaState, SchemaWorld } from \"./cyg-server\";\n",
        );
    } else {
        out.push_str("import { SchemaDef, SchemaEntity } from \"./cyg-server\";\n");
    }

    for schema in schemas {
        out.push('\n');
        out.push_str(&format!(
            "export const {}Schema = new SchemaDef(\"{}\", {})\n",
            schema.name, schema.name, schema.version
        ));
        for field in &schema.fields {
            emit_field_builder(&mut out, field);
        }
        // Replace the final newline with a semicolon on the last builder line.
        out.pop();
        out.push_str(";\n\n");

        for field in &schema.fields {
            out.push_str(&format!(
                "const _{schema}_{field}: i32 = {schema}Schema.indexOf(\"{field}\");\n",
                schema = schema.name,
                field = field.name
            ));
        }
        out.push('\n');

        out.push_str(&format!(
            "export class {} extends SchemaEntity {{\n",
            schema.name
        ));
        out.push_str("    constructor() {\n        super(");
        out.push_str(&schema.name);
        out.push_str("Schema);\n");
        for field in &schema.fields {
            if let Some(default) = &field.default {
                out.push_str(&format!(
                    "        this.set{}(_{schema}_{name}, {literal});\n",
                    field.kind.value_suffix(),
                    schema = schema.name,
                    name = field.name,
                    literal = default_literal(field.kind, default),
                ));
            }
        }
        out.push_str("    }\n");
        for field in &schema.fields {
            emit_accessors(&mut out, &format!("_{}", schema.name), field);
        }
        out.push_str("}\n");
    }

    if let Some(root) = root.filter(|root| is_multi_root(Some(root))) {
        emit_world(&mut out, root);
    }
    out
}

fn is_multi_root(root: Option<&RootState>) -> bool {
    root.map(|root| !root.scalars.is_empty() || root.collections.len() != 1)
        .unwrap_or(false)
}

/// Emit the typed root world for a `State` with scalars and/or several maps.
/// A single-map, scalar-free root keeps the legacy `SchemaState` wire.
fn emit_world(out: &mut String, root: &RootState) {
    out.push('\n');
    out.push_str(&format!(
        "export const {}Schema = new SchemaDef(\"{}\", {})\n",
        root.name, root.name, root.version
    ));
    for field in &root.scalars {
        emit_field_builder(out, field);
    }
    out.pop();
    out.push_str(";\n\n");

    for field in &root.scalars {
        out.push_str(&format!(
            "const _{root}_{field}: i32 = {root}Schema.indexOf(\"{field}\");\n",
            root = root.name,
            field = field.name
        ));
    }
    out.push('\n');

    out.push_str(&format!(
        "export class {}Root extends SchemaEntity {{\n    constructor() {{\n        super({}Schema);\n",
        root.name, root.name
    ));
    for field in &root.scalars {
        if let Some(default) = &field.default {
            out.push_str(&format!(
                "        this.set{}(_{root}_{name}, {literal});\n",
                field.kind.value_suffix(),
                root = root.name,
                name = field.name,
                literal = default_literal(field.kind, default),
            ));
        }
    }
    out.push_str("    }\n}\n\n");

    out.push_str("export class GameState extends SchemaWorld {\n");
    for collection in &root.collections {
        out.push_str(&format!(
            "    {name}: SchemaState;\n",
            name = collection.name
        ));
    }
    if !root.collections.is_empty() {
        out.push('\n');
    }
    out.push_str(&format!(
        "    constructor() {{\n        super(new {}Root());\n",
        root.name
    ));
    for collection in &root.collections {
        out.push_str(&format!(
            "        this.{name} = new SchemaState({schema}Schema);\n",
            name = collection.name,
            schema = collection.schema
        ));
    }
    for collection in &root.collections {
        out.push_str(&format!(
            "        this.add(this.{name});\n",
            name = collection.name
        ));
    }
    out.push_str("    }\n");

    for field in &root.scalars {
        let ty = match field.kind {
            WireKind::Array => {
                array_assemblyscript_type(field.element.unwrap_or(WireKind::F64))
            }
            other => other.assemblyscript_type(),
        };
        let suffix = field.kind.value_suffix();
        out.push_str(&format!(
            "\n    get {name}(): {ty} {{\n        return this.root.get{suffix}(_{root}_{name});\n    }}\n",
            name = field.name,
            root = root.name
        ));
        out.push_str(&format!(
            "    set {name}(v: {ty}) {{\n        this.root.set{suffix}(_{root}_{name}, v);\n    }}\n",
            name = field.name,
            root = root.name
        ));
        if field.optional() {
            let cap = capitalize(&field.name);
            out.push_str(&format!(
                "\n    has{cap}(): bool {{\n        return this.root.hasField(_{root}_{name});\n    }}\n",
                name = field.name,
                root = root.name
            ));
            out.push_str(&format!(
                "    clear{cap}(): void {{\n        this.root.clearField(_{root}_{name});\n    }}\n",
                root = root.name,
                name = field.name
            ));
        }
    }
    out.push_str("}\n");
}

fn field_typescript_type(field: &SchemaField) -> String {
    let base = match field.kind {
        WireKind::Array => array_typescript_type(field.element.unwrap_or(WireKind::F64)),
        WireKind::String => "string",
        other => other.typescript_type(),
    };
    if field.optional() {
        format!("{base} | null")
    } else {
        base.to_string()
    }
}

fn emit_client_dts(
    schemas: &[SchemaDef],
    root: Option<&RootState>,
    source_name: &str,
) -> String {
    let mut out = String::new();
    out.push_str(HEADER);
    out.push('\n');
    out.push_str(&format!(
        "// Generated from {source_name} by `ox-game schema`. Edit the schema source, not this file.\n\n"
    ));
    out.push_str(
        "import type { Room as CYGRoom, SchemaDef, SchemaEntity, SchemaMap } from \"./cyg-client\";\n",
    );

    for schema in schemas {
        out.push('\n');
        out.push_str(&format!(
            "export declare const {}Schema: SchemaDef;\n",
            schema.name
        ));
        out.push_str(&format!(
            "/** One decoded row from `{}Schema` (wire version {}). */\n",
            schema.name, schema.version
        ));
        out.push_str(&format!("export interface {}State {{\n", schema.name));
        out.push_str("    id: number;\n");
        for field in &schema.fields {
            out.push_str(&format!(
                "    {name}: {ty};\n",
                name = field.name,
                ty = field_typescript_type(field)
            ));
        }
        out.push_str("}\n");
    }

    // Each collection is a Map on the runtime and a named property on the
    // typed state, so `room.state.players.get(id)` type-checks. Root scalars
    // are plain properties of `room.state`.
    if let Some(root) = root {
        out.push('\n');
        out.push_str("/** Typed root state exposed as `room.state`. */\n");
        out.push_str("export interface GameState {\n");
        for field in &root.scalars {
            out.push_str(&format!(
                "    {name}: {ty};\n",
                name = field.name,
                ty = field_typescript_type(field)
            ));
        }
        for collection in &root.collections {
            let row = schemas
                .iter()
                .find(|schema| schema.name == collection.schema)
                .map(|schema| format!("{}State", schema.name))
                .unwrap_or_else(|| "SchemaEntity".to_string());
            out.push_str(&format!(
                "    {name}: SchemaMap<{row}>;\n",
                name = collection.name,
                row = row
            ));
        }
        out.push_str("}\n");
        out.push_str("\n/** Convenience alias for a room carrying this schema. */\n");
        out.push_str("export type GameRoom = CYGRoom<GameState>;\n");
    }

    out.push_str("\nexport declare const schemas: {\n");
    for schema in schemas {
        out.push_str(&format!("    {0}: SchemaDef;\n", schema.name));
    }
    out.push_str("};\n");

    out.push_str("\ndeclare global {\n    interface Window {\n        CYGSchema: {\n");
    for schema in schemas {
        out.push_str(&format!("            {0}Schema: SchemaDef;\n", schema.name));
    }
    out.push_str("            schemas: typeof schemas;\n        };\n    }\n}\n");

    // `schema.ts` is authored with Colyseus imports but is parsed, not built.
    // This ambient module lets a plain TypeScript project type-check the source
    // without installing @colyseus/schema. A real installation wins by
    // declaration merging and remains compatible.
    out.push_str(
        "\n/** Type shim for `schema.ts` when cyg is not installed. */\n\
         declare module \"cyg\" {\n\
         \x20   export interface SchemaType<T> {\n\
         \x20       extend<B extends Record<string, unknown>>(base: B): SchemaType<T & B> & (T & B);\n\
         \x20       version(value: number): SchemaType<T> & T;\n\
         \x20   }\n\
         \x20   export function schema<T extends Record<string, unknown>>(fields: T, name?: string, base?: unknown): SchemaType<T> & T;\n\
         \x20   export const t: {\n\
         \x20       number(): any;\n\
         \x20       float64(): any;\n\
         \x20       float32(): any;\n\
         \x20       int8(): any;\n\
         \x20       uint8(): any;\n\
         \x20       int16(): any;\n\
         \x20       uint16(): any;\n\
         \x20       int32(): any;\n\
         \x20       uint32(): any;\n\
         \x20       int64(): any;\n\
         \x20       boolean(): any;\n\
         \x20       bool(): any;\n\
         \x20       string(): any;\n\
         \x20       map<T>(target: T): any;\n\
         \x20       ref<T>(target: T): any;\n\
         \x20       array<T>(target: T): any;\n\
         \x20       set<T>(target: T): any;\n\
         \x20       collection<T>(target: T): any;\n\
         \x20       [key: string]: any;\n\
         \x20   };\n\
         }\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_matches_the_js_and_assemblyscript_runtimes() {
        let parsed = ts::parse(
            r#"
            export const Player = schema({
                x: t.number(),
                y: t.number(),
                flags: t.uint32(),
            }, "Player");
            "#,
        )
        .unwrap();
        // Expected value produced by the browser SDK's `schemaFnv1a` chain.
        assert_eq!(wire_hash(&parsed.schemas[0]), 1_154_959_100);
    }

    #[test]
    fn descriptor_carries_schema_fields_hash_and_root_collection() {
        let parsed = ts::parse(
            r#"
            export const Player = schema({
                x: t.number(),
                y: t.number(),
                flags: t.uint32(),
            }, "Player");
            "#,
        )
        .unwrap();
        let schemas = parsed.schemas;
        let root = RootState {
            name: "State".into(),
            version: 1,
            scalars: Vec::new(),
            collections: vec![RootCollection {
                name: "players".into(),
                schema: "Player".into(),
            }],
        };
        let bytes = emit_descriptor(&schemas, Some(&root));
        assert_eq!(&bytes[..4], b"OXSD");
        assert_eq!(bytes[4], DESCRIPTOR_VERSION);
        assert!(
            bytes.windows(8).any(|window| window == b"\x06\x00Player"),
            "schema name is length-prefixed"
        );
        assert!(
            bytes.windows(9).any(|window| window == b"\x07\x00players"),
            "root collection is recorded"
        );
        let descriptor = generate_with_root(&schemas, Some(&root), "schema.ts").descriptor;
        assert_eq!(descriptor, bytes);
    }

    #[test]
    fn ts_schema_generates_typed_class_defaults_and_descriptor() {
        let parsed = ts::parse(
            r#"
            export const Player = schema({
                x: t.number(),
                hp: t.uint8().default(100),
                flags: t.uint32(),
            }, "Player");
            export const State = schema({ players: t.map(Player) }, "State");
            "#,
        )
        .unwrap();
        let generated = generate_with_root(&parsed.schemas, parsed.root.as_ref(), "schema.ts");
        assert!(generated.server_ts.contains(".f64(\"x\")"));
        assert!(generated.server_ts.contains(".u8(\"hp\")"));
        assert!(generated.server_ts.contains("this.setU8(_Player_hp, 100);"));
        assert_eq!(&generated.descriptor[..4], b"OXSD");
    }
}
