//! Typed client bindings for the schema contract.
//!
//! `ox-game schema-codegen --lang cs|gdscript|cpp|lua` turns the same
//! `schema.ts` that drives the host into entity classes plus a field table for
//! one target language. The wire itself is unchanged: every runtime rebuilds
//! the field list from the `OXSD` descriptor the host sends on join, and these
//! files only give that data a native shape. They are metadata and accessors,
//! not a second source of truth.

use std::fmt::Write as _;

use crate::{RootState, SchemaDef, SchemaField, WireKind};

/// Target language for [`emit`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    CSharp,
    Gdscript,
    Cpp,
    Lua,
    /// AssemblyScript server-side state: typed entities + dirty tracking +
    /// `commit()` — the developer mutates fields, never touches bytes.
    As,
}

impl Language {
    pub const ALL: [Language; 5] = [
        Language::CSharp,
        Language::Gdscript,
        Language::Cpp,
        Language::Lua,
        Language::As,
    ];

    pub fn parse(text: &str) -> Option<Language> {
        match text {
            "cs" | "csharp" | "c#" => Some(Language::CSharp),
            "gd" | "gdscript" | "godot" => Some(Language::Gdscript),
            "cpp" | "c++" | "cxx" => Some(Language::Cpp),
            "lua" | "luajit" => Some(Language::Lua),
            "as" | "assemblyscript" => Some(Language::As),
            _ => None,
        }
    }

    /// File name written by `ox-game schema-codegen`.
    pub fn file_name(self) -> &'static str {
        match self {
            Language::CSharp => "OpenOxideSchemaGenerated.cs",
            Language::Gdscript => "openoxide_schema_generated.gd",
            Language::Cpp => "openoxide_schema_generated.hpp",
            Language::Lua => "openoxide_schema_generated.lua",
            Language::As => "openoxide.schema.ts",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Language::CSharp => "C#",
            Language::Gdscript => "GDScript",
            Language::Cpp => "C++",
            Language::Lua => "Lua",
            Language::As => "AssemblyScript",
        }
    }

    /// Short flag accepted by `--lang`.
    pub fn flag(self) -> &'static str {
        match self {
            Language::CSharp => "cs",
            Language::Gdscript => "gdscript",
            Language::Cpp => "cpp",
            Language::Lua => "lua",
            Language::As => "as",
        }
    }
}

/// Emit the bindings for one target language.
pub fn emit(
    language: Language,
    schemas: &[SchemaDef],
    root: Option<&RootState>,
    source_name: &str,
) -> String {
    let source_name = sanitize_source(source_name);
    let root = root.cloned();
    match language {
        Language::CSharp => emit_csharp(schemas, root.as_ref(), &source_name),
        Language::As => emit_as(schemas, root.as_ref(), &source_name),
        Language::Gdscript => emit_gdscript(schemas, root.as_ref(), &source_name),
        Language::Cpp => emit_cpp(schemas, root.as_ref(), &source_name),
        Language::Lua => emit_lua(schemas, root.as_ref(), &source_name),
    }
}

fn sanitize_source(source_name: &str) -> String {
    source_name.replace(['\r', '\n'], " ")
}

fn escape_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}

fn root_hash(root: &RootState) -> u32 {
    crate::wire_hash(&SchemaDef {
        name: root.name.clone(),
        version: root.version,
        fields: root.scalars.clone(),
        line: 0,
        column: 0,
    })
}

/// Root classes are named after the root schema, unless an entity already
/// claimed that name; then `Root` is appended so both can be emitted.
fn root_class_name(root: &RootState, schemas: &[SchemaDef]) -> String {
    if schemas.iter().any(|schema| schema.name == root.name) {
        format!("{}Root", root.name)
    } else {
        root.name.clone()
    }
}

fn kind_name(kind: WireKind) -> &'static str {
    match kind {
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

fn kind_size(field: &SchemaField) -> usize {
    field.kind.size()
}

// ---------------------------------------------------------------------------
// C#
// ---------------------------------------------------------------------------

fn cs_scalar(kind: WireKind) -> &'static str {
    match kind {
        WireKind::U8 => "byte",
        WireKind::I8 => "sbyte",
        WireKind::U16 => "ushort",
        WireKind::I16 => "short",
        WireKind::U32 => "uint",
        WireKind::I32 => "int",
        WireKind::F32 => "float",
        WireKind::F64 => "double",
        WireKind::Bool => "bool",
        WireKind::I64 => "long",
        WireKind::String => "string",
        WireKind::Array => "double[]",
    }
}

fn cs_base_type(field: &SchemaField) -> String {
    if field.kind == WireKind::Array {
        format!("{}[]", cs_scalar(field.element.unwrap_or(WireKind::F64)))
    } else {
        cs_scalar(field.kind).to_string()
    }
}

fn cs_type(field: &SchemaField) -> String {
    let base = cs_base_type(field);
    if field.optional() && !matches!(field.kind, WireKind::String | WireKind::Array) {
        format!("{base}?")
    } else {
        base
    }
}

fn cs_default(field: &SchemaField) -> String {
    if field.optional() {
        return "null".into();
    }
    match field.kind {
        WireKind::String => "string.Empty".into(),
        WireKind::Array => format!(
            "new {}[0]",
            cs_scalar(field.element.unwrap_or(WireKind::F64))
        ),
        WireKind::Bool => "false".into(),
        WireKind::F32 => "0.0f".into(),
        WireKind::F64 => "0.0".into(),
        _ => "0".into(),
    }
}

fn cs_keyword(name: &str) -> bool {
    matches!(
        name,
        "abstract"
            | "as"
            | "base"
            | "bool"
            | "break"
            | "byte"
            | "case"
            | "catch"
            | "char"
            | "checked"
            | "class"
            | "const"
            | "continue"
            | "decimal"
            | "default"
            | "delegate"
            | "do"
            | "double"
            | "else"
            | "enum"
            | "event"
            | "explicit"
            | "extern"
            | "false"
            | "finally"
            | "fixed"
            | "float"
            | "for"
            | "foreach"
            | "goto"
            | "if"
            | "implicit"
            | "in"
            | "int"
            | "interface"
            | "internal"
            | "is"
            | "lock"
            | "long"
            | "namespace"
            | "new"
            | "null"
            | "object"
            | "operator"
            | "out"
            | "override"
            | "params"
            | "private"
            | "protected"
            | "public"
            | "readonly"
            | "ref"
            | "return"
            | "sbyte"
            | "sealed"
            | "short"
            | "sizeof"
            | "stackalloc"
            | "static"
            | "string"
            | "struct"
            | "switch"
            | "this"
            | "throw"
            | "true"
            | "try"
            | "typeof"
            | "uint"
            | "ulong"
            | "unchecked"
            | "unsafe"
            | "ushort"
            | "using"
            | "virtual"
            | "void"
            | "volatile"
            | "while"
    )
}

fn cs_ident(name: &str) -> String {
    if cs_keyword(name) {
        format!("@{name}")
    } else {
        name.to_string()
    }
}

fn cs_field_table(fields: &[SchemaField]) -> String {
    let mut out = String::new();
    for field in fields {
        let _ = writeln!(
            out,
            "            new OxFieldDef(\"{}\", OxWireKind::{:?}, {}, OxWireKind::{:?}, {}),",
            escape_string(&field.name),
            field.kind,
            field.flags,
            field.element.unwrap_or(WireKind::U8),
            kind_size(field)
        );
        // `OxWireKind::U8` must read as `OxWireKind.U8`; fix the C# spelling.
    }
    out.replace("OxWireKind::", "OxWireKind.")
}

fn cs_field_read(field: &SchemaField, index: usize) -> String {
    let _ = index;
    if !field.optional() {
        return match field.kind {
            WireKind::String | WireKind::Array => {
                format!("({})value;", cs_base_type(field))
            }
            _ => format!("({})value;", cs_base_type(field)),
        };
    }
    match field.kind {
        WireKind::String | WireKind::Array => format!("({})value;", cs_base_type(field)),
        _ => format!(
            "value == null ? ({ty})null : ({base})value;",
            ty = cs_type(field),
            base = cs_base_type(field)
        ),
    }
}

fn emit_cs_entity(
    out: &mut String,
    name: &str,
    fields: &[SchemaField],
    version: u32,
    hash: u32,
    doc: &str,
) {
    let _ = writeln!(out, "    /// <summary>{doc}</summary>");
    let _ = writeln!(out, "    public sealed class {name} : OxSchemaEntity");
    out.push_str("    {\n");
    let _ = writeln!(out, "        public const uint SchemaVersion = {version}u;");
    let _ = writeln!(out, "        public const uint SchemaHash = {hash}u;");
    let _ = writeln!(
        out,
        "        public const int FieldCount = {};",
        fields.len()
    );
    let _ = writeln!(
        out,
        "        public static readonly OxFieldDef[] Fields = new OxFieldDef[FieldCount]"
    );
    out.push_str("        {\n");
    out.push_str(&cs_field_table(fields));
    out.push_str("        };\n\n");

    for field in fields {
        let _ = writeln!(
            out,
            "        public {} {} = {};",
            cs_type(field),
            cs_ident(&field.name),
            cs_default(field)
        );
    }
    if !fields.is_empty() {
        out.push('\n');
    }

    out.push_str("        public override int OxFieldCount { get { return FieldCount; } }\n\n");
    out.push_str("        public override void OxSet(int field, object value)\n        {\n");
    out.push_str("            switch (field)\n            {\n");
    for (index, field) in fields.iter().enumerate() {
        let _ = writeln!(
            out,
            "                case {index}: {} = {} break;",
            cs_ident(&field.name),
            cs_field_read(field, index)
        );
    }
    out.push_str("            }\n        }\n\n");

    out.push_str("        public override object OxGet(int field)\n        {\n");
    out.push_str("            switch (field)\n            {\n");
    for (index, field) in fields.iter().enumerate() {
        let _ = writeln!(
            out,
            "                case {index}: return {};",
            cs_ident(&field.name)
        );
    }
    out.push_str("            }\n            return null;\n        }\n\n");

    out.push_str("        public override void OxClear()\n        {\n");
    for field in fields {
        let _ = writeln!(
            out,
            "            {} = {};",
            cs_ident(&field.name),
            cs_default(field)
        );
    }
    out.push_str("        }\n    }\n\n");
}

fn emit_csharp(schemas: &[SchemaDef], root: Option<&RootState>, source_name: &str) -> String {
    let mut out = String::new();
    out.push_str("// <auto-generated>\n");
    let _ = writeln!(
        out,
        "// OpenOxide schema bindings generated from {source_name}. Do not edit."
    );
    let _ = writeln!(out, "// Regenerate: ox-game schema-codegen . --lang cs");
    out.push_str("// </auto-generated>\n");
    out.push_str("namespace OpenOxide.Schema.Generated\n{\n");
    out.push_str("    using System.Collections.Generic;\n\n");

    for schema in schemas {
        emit_cs_entity(
            &mut out,
            &schema.name,
            &schema.fields,
            schema.version,
            crate::wire_hash(schema),
            &format!(
                "{} state entity (version {}, hash 0x{:08x}).",
                schema.name,
                schema.version,
                crate::wire_hash(schema)
            ),
        );
    }

    let root_class = root.map(|root| root_class_name(root, schemas));
    if let Some((root, class)) = root.zip(root_class.as_ref()) {
        out.push_str("    /// <summary>Root scalar state carried by the world frame.</summary>\n");
        emit_cs_entity(
            &mut out,
            class,
            &root.scalars,
            root.version,
            root_hash(root),
            &format!("Root state \"{}\" (version {}).", root.name, root.version),
        );
    }

    out.push_str("    public static class OpenOxideSchemaGenerated\n    {\n");
    let _ = writeln!(
        out,
        "        public const string SourceName = \"{}\";",
        escape_string(source_name)
    );
    if let Some(root) = root {
        let class = root_class_name(root, schemas);
        let _ = writeln!(
            out,
            "        public const string RootName = \"{}\";",
            escape_string(&root.name)
        );
        let _ = writeln!(
            out,
            "        public const uint RootVersion = {}u;",
            root.version
        );
        let _ = writeln!(
            out,
            "        public const uint RootHash = {}u;",
            root_hash(root)
        );
        let _ = writeln!(
            out,
            "        public static readonly OxFieldDef[] RootFields = {class}.Fields;"
        );
        out.push_str(
            "        public static readonly OxSchemaCollectionDef[] Collections = new OxSchemaCollectionDef[]\n        {\n",
        );
        for collection in &root.collections {
            let _ = writeln!(
                out,
                "            new OxSchemaCollectionDef(\"{}\", \"{}\"),",
                escape_string(&collection.name),
                escape_string(&collection.schema)
            );
        }
        out.push_str("        };\n");
    } else {
        out.push_str("        public const string RootName = \"\";\n");
        out.push_str("        public const uint RootVersion = 0u;\n");
        out.push_str("        public const uint RootHash = 0u;\n");
        out.push_str(
            "        public static readonly OxFieldDef[] RootFields = new OxFieldDef[0];\n",
        );
        out.push_str(
            "        public static readonly OxSchemaCollectionDef[] Collections = new OxSchemaCollectionDef[0];\n",
        );
    }

    out.push_str(
        "        public static readonly Dictionary<string, OxFieldDef[]> Schemas = new Dictionary<string, OxFieldDef[]>\n        {\n",
    );
    for schema in schemas {
        let _ = writeln!(
            out,
            "            {{ \"{}\", {}.Fields }},",
            escape_string(&schema.name),
            schema.name
        );
    }
    out.push_str("        };\n\n");
    out.push_str("        public static void Register(OxSchemaRegistry registry)\n        {\n");
    for schema in schemas {
        let _ = writeln!(
            out,
            "            registry.Register(\"{}\", delegate {{ return new {}(); }});",
            escape_string(&schema.name),
            schema.name
        );
    }
    if let Some(root) = root {
        let class = root_class_name(root, schemas);
        let _ = writeln!(
            out,
            "            registry.Register(\"{}\", delegate {{ return new {}(); }});",
            escape_string(&root.name),
            class
        );
    }
    out.push_str("        }\n    }\n}\n");
    out
}

// ---------------------------------------------------------------------------
// GDScript
// ---------------------------------------------------------------------------

fn gd_scalar(kind: WireKind) -> &'static str {
    match kind {
        WireKind::F32 | WireKind::F64 => "float",
        WireKind::Bool => "bool",
        WireKind::String => "String",
        WireKind::Array => "Array",
        _ => "int",
    }
}

fn gd_type(field: &SchemaField) -> &'static str {
    gd_scalar(field.kind)
}

fn gd_default(field: &SchemaField) -> &'static str {
    if field.optional() {
        return "null";
    }
    match field.kind {
        WireKind::String => "\"\"",
        WireKind::Array => "[]",
        WireKind::Bool => "false",
        WireKind::F32 | WireKind::F64 => "0.0",
        _ => "0",
    }
}

fn gd_keyword(name: &str) -> bool {
    matches!(
        name,
        "if" | "elif"
            | "else"
            | "for"
            | "while"
            | "match"
            | "when"
            | "break"
            | "continue"
            | "pass"
            | "return"
            | "class"
            | "class_name"
            | "extends"
            | "is"
            | "in"
            | "as"
            | "self"
            | "super"
            | "signal"
            | "func"
            | "static"
            | "const"
            | "enum"
            | "var"
            | "breakpoint"
            | "preload"
            | "await"
            | "yield"
            | "assert"
            | "void"
            | "true"
            | "false"
            | "null"
            | "and"
            | "or"
            | "not"
            | "export"
            | "onready"
            | "tool"
            | "remote"
            | "master"
            | "puppet"
            | "remotesync"
            | "mastersync"
            | "puppetsync"
            | "sync"
    )
}

fn gd_ident(name: &str) -> String {
    if gd_keyword(name) {
        format!("{name}_")
    } else {
        name.to_string()
    }
}

fn gd_convert(kind: WireKind) -> &'static str {
    match kind {
        WireKind::F32 | WireKind::F64 => "float",
        WireKind::Bool => "bool",
        WireKind::String => "str",
        WireKind::Array => "",
        _ => "int",
    }
}

fn gd_field_meta(fields: &[SchemaField], indent: &str) -> String {
    let mut out = String::new();
    for field in fields {
        let element = field.element.map(kind_name).unwrap_or("none");
        let _ = writeln!(
            out,
            "{indent}{{\"name\": \"{}\", \"kind\": \"{}\", \"code\": {}, \"flags\": {}, \"element\": \"{}\", \"size\": {}}},",
            escape_string(&field.name),
            kind_name(field.kind),
            field.kind.code(),
            field.flags,
            element,
            kind_size(field)
        );
    }
    out
}

fn emit_gd_entity(out: &mut String, name: &str, fields: &[SchemaField], version: u32, hash: u32) {
    let _ = writeln!(out, "class {name} extends OpenOxideSchema.Entity:");
    let _ = writeln!(out, "    const SCHEMA_VERSION := {version}");
    let _ = writeln!(out, "    const SCHEMA_HASH := {hash}");
    if fields.is_empty() {
        out.push_str("    pass\n\n");
        return;
    }
    for field in fields {
        if field.optional() {
            // Optional fields are absent from the wire, so they cannot carry a
            // static type: `null` is the only value that says "not sent".
            let _ = writeln!(out, "    var {} = null", gd_ident(&field.name));
        } else {
            let _ = writeln!(
                out,
                "    var {}: {} = {}",
                gd_ident(&field.name),
                gd_type(field),
                gd_default(field)
            );
        }
    }
    out.push('\n');

    out.push_str("    func set_field(field: int, value) -> void:\n");
    out.push_str("        match field:\n");
    for (index, field) in fields.iter().enumerate() {
        let ident = gd_ident(&field.name);
        let convert = gd_convert(field.kind);
        let _ = writeln!(out, "            {index}:");
        if field.optional() {
            let _ = writeln!(
                out,
                "                {ident} = null if value == null else {}(value)",
                if convert.is_empty() { "value" } else { convert }
            );
        } else if convert.is_empty() {
            let _ = writeln!(out, "                {ident} = value");
        } else {
            let _ = writeln!(out, "                {ident} = {convert}(value)");
        }
    }
    out.push_str("            _:\n                pass\n\n");

    out.push_str("    func get_field(field: int):\n");
    out.push_str("        match field:\n");
    for (index, field) in fields.iter().enumerate() {
        let _ = writeln!(
            out,
            "            {index}:\n                return {}",
            gd_ident(&field.name)
        );
    }
    out.push_str("        return null\n\n");

    out.push_str("    func clear_fields() -> void:\n");
    for field in fields {
        let _ = writeln!(
            out,
            "        {} = {}",
            gd_ident(&field.name),
            gd_default(field)
        );
    }
    out.push('\n');
}

fn emit_gdscript(schemas: &[SchemaDef], root: Option<&RootState>, source_name: &str) -> String {
    let mut out = String::new();
    out.push_str("# <auto-generated>\n");
    let _ = writeln!(
        out,
        "# OpenOxide schema bindings generated from {source_name}. Do not edit."
    );
    out.push_str("# Regenerate: ox-game schema-codegen . --lang gdscript\n");
    out.push_str("# </auto-generated>\n");
    out.push_str("class_name OpenOxideSchemaGenerated\nextends RefCounted\n\n");
    let _ = writeln!(
        out,
        "const SOURCE_NAME := \"{}\"",
        escape_string(source_name)
    );
    out.push('\n');

    for schema in schemas {
        emit_gd_entity(
            &mut out,
            &schema.name,
            &schema.fields,
            schema.version,
            crate::wire_hash(schema),
        );
    }

    if let Some(root) = root {
        emit_gd_entity(
            &mut out,
            &root_class_name(root, schemas),
            &root.scalars,
            root.version,
            root_hash(root),
        );
    }

    out.push_str("const SCHEMAS := {\n");
    for schema in schemas {
        let _ = writeln!(
            out,
            "    \"{}\": {{\n        \"version\": {},\n        \"hash\": {},\n        \"fields\": [",
            escape_string(&schema.name),
            schema.version,
            crate::wire_hash(schema)
        );
        out.push_str(&gd_field_meta(&schema.fields, "            "));
        out.push_str("        ],\n    },\n");
    }
    out.push_str("}\n\n");

    if let Some(root) = root {
        let _ = writeln!(out, "const ROOT_NAME := \"{}\"", escape_string(&root.name));
        let _ = writeln!(out, "const ROOT_VERSION := {}", root.version);
        let _ = writeln!(out, "const ROOT_HASH := {}", root_hash(root));
        out.push_str("const ROOT_FIELDS := [\n");
        out.push_str(&gd_field_meta(&root.scalars, "    "));
        out.push_str("]\n");
        out.push_str("const ROOT_COLLECTIONS := [\n");
        for collection in &root.collections {
            let _ = writeln!(
                out,
                "    {{\"name\": \"{}\", \"schema\": \"{}\"}},",
                escape_string(&collection.name),
                escape_string(&collection.schema)
            );
        }
        out.push_str("]\n\n");
    } else {
        out.push_str("const ROOT_NAME := \"\"\nconst ROOT_VERSION := 0\nconst ROOT_HASH := 0\n");
        out.push_str("const ROOT_FIELDS := []\nconst ROOT_COLLECTIONS := []\n\n");
    }

    out.push_str("static func register(registry) -> void:\n");
    for schema in schemas {
        let _ = writeln!(
            out,
            "    registry.register(\"{}\", func(): return {}.new())",
            escape_string(&schema.name),
            schema.name
        );
    }
    if let Some(root) = root {
        let class = root_class_name(root, schemas);
        let _ = writeln!(
            out,
            "    registry.register(\"{}\", func(): return {}.new())",
            escape_string(&root.name),
            class
        );
    }
    if schemas.is_empty() && root.is_none() {
        out.push_str("    pass\n");
    }
    out
}

// ---------------------------------------------------------------------------
// C++
// ---------------------------------------------------------------------------

fn cpp_scalar(kind: WireKind) -> &'static str {
    match kind {
        WireKind::U8 => "uint8_t",
        WireKind::I8 => "int8_t",
        WireKind::U16 => "uint16_t",
        WireKind::I16 => "int16_t",
        WireKind::U32 => "uint32_t",
        WireKind::I32 => "int32_t",
        WireKind::F32 => "float",
        WireKind::F64 => "double",
        WireKind::Bool => "bool",
        WireKind::I64 => "int64_t",
        WireKind::String => "std::string",
        WireKind::Array => "std::vector<Value>",
    }
}

fn cpp_base_type(field: &SchemaField) -> String {
    if field.kind == WireKind::Array {
        format!(
            "std::vector<{}>",
            cpp_scalar(field.element.unwrap_or(WireKind::F64))
        )
    } else {
        cpp_scalar(field.kind).to_string()
    }
}

fn cpp_type(field: &SchemaField) -> String {
    let base = cpp_base_type(field);
    if field.optional() {
        format!("std::optional<{base}>")
    } else {
        base
    }
}

fn cpp_default(field: &SchemaField) -> &'static str {
    if field.optional() {
        return "std::nullopt";
    }
    match field.kind {
        WireKind::Bool => "false",
        WireKind::F32 => "0.0f",
        WireKind::F64 => "0.0",
        _ => "{}",
    }
}

fn cpp_keyword(name: &str) -> bool {
    matches!(
        name,
        "alignas"
            | "alignof"
            | "and"
            | "and_eq"
            | "asm"
            | "auto"
            | "bitand"
            | "bitor"
            | "bool"
            | "break"
            | "case"
            | "catch"
            | "char"
            | "char8_t"
            | "char16_t"
            | "char32_t"
            | "class"
            | "compl"
            | "concept"
            | "const"
            | "consteval"
            | "constexpr"
            | "constinit"
            | "const_cast"
            | "continue"
            | "co_await"
            | "co_return"
            | "co_yield"
            | "decltype"
            | "default"
            | "delete"
            | "do"
            | "double"
            | "dynamic_cast"
            | "else"
            | "enum"
            | "explicit"
            | "export"
            | "extern"
            | "false"
            | "float"
            | "for"
            | "friend"
            | "goto"
            | "if"
            | "inline"
            | "int"
            | "long"
            | "mutable"
            | "namespace"
            | "new"
            | "noexcept"
            | "not"
            | "not_eq"
            | "nullptr"
            | "operator"
            | "or"
            | "or_eq"
            | "private"
            | "protected"
            | "public"
            | "register"
            | "reinterpret_cast"
            | "requires"
            | "return"
            | "short"
            | "signed"
            | "sizeof"
            | "static"
            | "static_assert"
            | "static_cast"
            | "struct"
            | "switch"
            | "template"
            | "this"
            | "thread_local"
            | "throw"
            | "true"
            | "try"
            | "typedef"
            | "typeid"
            | "typename"
            | "union"
            | "unsigned"
            | "using"
            | "virtual"
            | "void"
            | "volatile"
            | "wchar_t"
            | "while"
            | "xor"
            | "xor_eq"
    )
}

fn cpp_ident(name: &str) -> String {
    if cpp_keyword(name) {
        format!("{name}_")
    } else {
        name.to_string()
    }
}

fn cpp_field_table(fields: &[SchemaField]) -> String {
    let mut out = String::new();
    for field in fields {
        let _ = writeln!(
            out,
            "        {{\"{}\", WireKind::{:?}, {}, WireKind::{:?}, {}}},",
            escape_string(&field.name),
            field.kind,
            field.flags,
            field.element.unwrap_or(WireKind::U8),
            kind_size(field)
        );
    }
    out
}

fn cpp_value_make(kind: WireKind, expr: &str) -> String {
    match kind {
        WireKind::U8 => format!("Value::make_u8({expr})"),
        WireKind::I8 => format!("Value::make_i8({expr})"),
        WireKind::U16 => format!("Value::make_u16({expr})"),
        WireKind::I16 => format!("Value::make_i16({expr})"),
        WireKind::U32 => format!("Value::make_u32({expr})"),
        WireKind::I32 => format!("Value::make_i32({expr})"),
        WireKind::F32 => format!("Value::make_f32({expr})"),
        WireKind::F64 => format!("Value::make_f64({expr})"),
        WireKind::Bool => format!("Value::make_bool({expr})"),
        WireKind::I64 => format!("Value::make_i64({expr})"),
        WireKind::String => format!("Value::make_string({expr})"),
        WireKind::Array => format!("Value::make_array({expr})"),
    }
}

fn cpp_value_read(kind: WireKind, expr: &str) -> String {
    match kind {
        WireKind::U8 => format!("{expr}.as_u8()"),
        WireKind::I8 => format!("{expr}.as_i8()"),
        WireKind::U16 => format!("{expr}.as_u16()"),
        WireKind::I16 => format!("{expr}.as_i16()"),
        WireKind::U32 => format!("{expr}.as_u32()"),
        WireKind::I32 => format!("{expr}.as_i32()"),
        WireKind::F32 => format!("{expr}.as_f32()"),
        WireKind::F64 => format!("{expr}.as_f64()"),
        WireKind::Bool => format!("{expr}.as_bool()"),
        WireKind::I64 => format!("{expr}.as_i64()"),
        WireKind::String => format!("{expr}.as_string()"),
        WireKind::Array => format!("{expr}.as_array()"),
    }
}

fn cpp_set_case(field: &SchemaField, ident: &str) -> String {
    let mut out = String::new();
    if field.kind == WireKind::Array {
        let element = field.element.unwrap_or(WireKind::F64);
        let read = cpp_value_read(element, "item");
        out.push_str("            case {index}: {\n");
        if field.optional() {
            out.push_str("                if (value.is_null()) { ");
            let _ = writeln!(out, "{ident}.reset(); break; }}");
            let _ = writeln!(
                out,
                "                std::vector<{}> next;",
                cpp_scalar(element)
            );
        } else {
            let _ = writeln!(out, "                {ident}.clear();");
            let _ = writeln!(
                out,
                "                std::vector<{}> next;",
                cpp_scalar(element)
            );
        }
        out.push_str("                const auto& items = value.as_array();\n");
        out.push_str("                next.reserve(items.size());\n");
        let _ = writeln!(
            out,
            "                for (const auto& item : items) next.push_back({read});"
        );
        if field.optional() {
            let _ = writeln!(out, "                {ident} = std::move(next);");
        } else {
            let _ = writeln!(out, "                {ident} = std::move(next);");
        }
        out.push_str("                break;\n            }\n");
    } else if field.optional() {
        let read = cpp_value_read(field.kind, "value");
        let _ = writeln!(
            out,
            "            case {{index}}: {ident} = value.is_null() ? std::nullopt : std::optional<{}>({}); break;",
            cpp_base_type(field),
            read
        );
    } else {
        let read = cpp_value_read(field.kind, "value");
        let _ = writeln!(out, "            case {{index}}: {ident} = {read}; break;");
    }
    out
}

fn cpp_get_case(field: &SchemaField, ident: &str, index: usize) -> String {
    let mut out = String::new();
    if field.kind == WireKind::Array {
        let element = field.element.unwrap_or(WireKind::F64);
        let _ = writeln!(out, "            case {index}: {{");
        if field.optional() {
            let _ = writeln!(out, "                if (!{ident}) return Value::null();");
            out.push_str("                std::vector<Value> items;\n");
            let _ = writeln!(out, "                items.reserve({ident}->size());");
            let _ = writeln!(
                out,
                "                for (const auto& item : *{ident}) items.push_back({});",
                cpp_value_make(element, "item")
            );
            out.push_str(
                "                return Value::make_array(std::move(items));\n            }\n",
            );
        } else {
            out.push_str("                std::vector<Value> items;\n");
            let _ = writeln!(out, "                items.reserve({ident}.size());");
            let _ = writeln!(
                out,
                "                for (const auto& item : {ident}) items.push_back({});",
                cpp_value_make(element, "item")
            );
            out.push_str(
                "                return Value::make_array(std::move(items));\n            }\n",
            );
        }
    } else if field.optional() {
        let _ = writeln!(out, "            case {index}:");
        let _ = writeln!(
            out,
            "                return {ident} ? {} : Value::null();",
            cpp_value_make(field.kind, format!("*{ident}").as_str())
        );
    } else {
        let _ = writeln!(
            out,
            "            case {index}: return {};",
            cpp_value_make(field.kind, ident)
        );
    }
    out
}

fn emit_cpp_entity(
    out: &mut String,
    name: &str,
    fields: &[SchemaField],
    version: u32,
    hash: u32,
    doc: &str,
) {
    let _ = writeln!(out, "/// {doc}");
    let _ = writeln!(out, "struct {name} final : public Entity {{");
    let _ = writeln!(
        out,
        "    inline static constexpr uint32_t kVersion = {version}u;"
    );
    let _ = writeln!(out, "    inline static constexpr uint32_t kHash = {hash}u;");
    out.push_str("    inline static const std::vector<FieldDef> kFields = {\n");
    out.push_str(&cpp_field_table(fields));
    out.push_str("    };\n\n");
    for field in fields {
        let _ = writeln!(
            out,
            "    {} {} = {};",
            cpp_type(field),
            cpp_ident(&field.name),
            cpp_default(field)
        );
    }
    if !fields.is_empty() {
        out.push('\n');
    }
    out.push_str(
        "    int field_count() const override { return static_cast<int>(kFields.size()); }\n\n",
    );
    out.push_str("    void set_field(int field, const Value& value) override {\n");
    out.push_str("        switch (field) {\n");
    for (index, field) in fields.iter().enumerate() {
        let case = cpp_set_case(field, &cpp_ident(&field.name));
        out.push_str(&case.replace("{index}", &index.to_string()));
    }
    out.push_str("            default: break;\n        }\n    }\n\n");
    out.push_str("    Value get_field(int field) const override {\n");
    out.push_str("        switch (field) {\n");
    for (index, field) in fields.iter().enumerate() {
        out.push_str(&cpp_get_case(field, &cpp_ident(&field.name), index));
    }
    out.push_str("            default: return Value::null();\n        }\n    }\n\n");
    out.push_str("    void clear_fields() override {\n");
    for field in fields {
        let _ = writeln!(
            out,
            "        {} = {};",
            cpp_ident(&field.name),
            cpp_default(field)
        );
    }
    out.push_str("    }\n};\n\n");
}

fn emit_cpp(schemas: &[SchemaDef], root: Option<&RootState>, source_name: &str) -> String {
    let mut out = String::new();
    out.push_str("// <auto-generated>\n");
    let _ = writeln!(
        out,
        "// OpenOxide schema bindings generated from {source_name}. Do not edit."
    );
    out.push_str("// Regenerate: ox-game schema-codegen . --lang cpp\n");
    out.push_str("// </auto-generated>\n");
    out.push_str("#pragma once\n\n");
    out.push_str("#include <cstdint>\n#include <memory>\n#include <optional>\n#include <string>\n#include <utility>\n#include <vector>\n\n");
    out.push_str("#include \"openoxide_schema_runtime.hpp\"\n\n");
    out.push_str("namespace openoxide::schema::generated {\n\n");

    for schema in schemas {
        emit_cpp_entity(
            &mut out,
            &schema.name,
            &schema.fields,
            schema.version,
            crate::wire_hash(schema),
            &format!(
                "{} state entity (version {}, hash 0x{:08x}).",
                schema.name,
                schema.version,
                crate::wire_hash(schema)
            ),
        );
    }
    if let Some(root) = root {
        emit_cpp_entity(
            &mut out,
            &root_class_name(root, schemas),
            &root.scalars,
            root.version,
            root_hash(root),
            &format!("Root state \"{}\" (version {}).", root.name, root.version),
        );
    }

    out.push_str("inline const std::vector<FieldDef>& root_fields() {\n");
    out.push_str("    static const std::vector<FieldDef> fields = {\n");
    if let Some(root) = root {
        out.push_str(&cpp_field_table(&root.scalars));
    }
    out.push_str("    };\n    return fields;\n}\n\n");

    out.push_str(
        "inline const std::vector<std::pair<std::string, std::string>>& root_collections() {\n",
    );
    out.push_str(
        "    static const std::vector<std::pair<std::string, std::string>> collections = {\n",
    );
    if let Some(root) = root {
        for collection in &root.collections {
            let _ = writeln!(
                out,
                "        {{\"{}\", \"{}\"}},",
                escape_string(&collection.name),
                escape_string(&collection.schema)
            );
        }
    }
    out.push_str("    };\n    return collections;\n}\n\n");

    if let Some(root) = root {
        let _ = writeln!(
            out,
            "inline const char* root_name() {{ return \"{}\"; }}",
            escape_string(&root.name)
        );
        let _ = writeln!(
            out,
            "inline uint32_t root_version() {{ return {}u; }}",
            root.version
        );
        let _ = writeln!(
            out,
            "inline uint32_t root_hash_value() {{ return {}u; }}",
            root_hash(root)
        );
    } else {
        out.push_str("inline const char* root_name() { return \"\"; }\n");
        out.push_str("inline uint32_t root_version() { return 0u; }\n");
        out.push_str("inline uint32_t root_hash_value() { return 0u; }\n");
    }
    out.push('\n');

    out.push_str("inline void register_generated(Registry& registry) {\n");
    for schema in schemas {
        let _ = writeln!(
            out,
            "    registry.add(\"{}\", []() -> std::unique_ptr<Entity> {{ return std::make_unique<{}>(); }});",
            escape_string(&schema.name),
            schema.name
        );
    }
    if let Some(root) = root {
        let class = root_class_name(root, schemas);
        let _ = writeln!(
            out,
            "    registry.add(\"{}\", []() -> std::unique_ptr<Entity> {{ return std::make_unique<{}>(); }});",
            escape_string(&root.name),
            class
        );
    }
    out.push_str("}\n\n}\n");
    out
}

// ---------------------------------------------------------------------------
// Lua
// ---------------------------------------------------------------------------

fn lua_field_table(fields: &[SchemaField], indent: &str) -> String {
    let mut out = String::new();
    for field in fields {
        let element = field
            .element
            .map(|kind| format!("\"{}\"", kind_name(kind)))
            .unwrap_or_else(|| "nil".into());
        let _ = writeln!(
            out,
            "{indent}{{ name = \"{}\", kind = \"{}\", code = {}, flags = {}, element = {}, size = {} }},",
            escape_string(&field.name),
            kind_name(field.kind),
            field.kind.code(),
            field.flags,
            element,
            kind_size(field)
        );
    }
    out
}

fn emit_lua(schemas: &[SchemaDef], root: Option<&RootState>, source_name: &str) -> String {
    let mut out = String::new();
    out.push_str("-- <auto-generated>\n");
    let _ = writeln!(
        out,
        "-- OpenOxide schema bindings generated from {source_name}. Do not edit."
    );
    out.push_str("-- Regenerate: ox-game schema-codegen . --lang lua\n");
    out.push_str("-- </auto-generated>\n");
    out.push_str("local OpenOxideSchema = require(\"openoxide_schema\")\n\n");
    out.push_str("local Generated = {}\n\n");
    let _ = writeln!(out, "Generated.source = \"{}\"", escape_string(source_name));
    out.push('\n');

    out.push_str("Generated.schemas = {\n");
    for schema in schemas {
        let _ = writeln!(
            out,
            "    {} = {{\n        name = \"{}\",\n        version = {},\n        hash = {},\n        fields = {{",
            schema.name,
            escape_string(&schema.name),
            schema.version,
            crate::wire_hash(schema)
        );
        out.push_str(&lua_field_table(&schema.fields, "            "));
        out.push_str("        },\n    },\n");
    }
    out.push_str("}\n\n");

    if let Some(root) = root {
        out.push_str("Generated.root = {\n");
        let _ = writeln!(out, "    name = \"{}\",", escape_string(&root.name));
        let _ = writeln!(out, "    version = {},", root.version);
        let _ = writeln!(out, "    hash = {},", root_hash(root));
        out.push_str("    fields = {\n");
        out.push_str(&lua_field_table(&root.scalars, "        "));
        out.push_str("    },\n    collections = {\n");
        for collection in &root.collections {
            let _ = writeln!(
                out,
                "        {{ name = \"{}\", schema = \"{}\" }},",
                escape_string(&collection.name),
                escape_string(&collection.schema)
            );
        }
        out.push_str("    },\n}\n\n");

        let _ = writeln!(
            out,
            "function Generated.new_{}()",
            root_class_name(root, schemas)
        );
        out.push_str("    return OpenOxideSchema.new_entity(Generated.root)\nend\n\n");
    } else {
        out.push_str("Generated.root = { name = \"\", version = 0, hash = 0, fields = {}, collections = {} }\n\n");
    }

    for schema in schemas {
        let _ = writeln!(out, "function Generated.new_{}()", schema.name);
        let _ = writeln!(
            out,
            "    return OpenOxideSchema.new_entity(Generated.schemas.{})",
            schema.name
        );
        out.push_str("end\n\n");
    }

    out.push_str("function Generated.defaults(schema_name)\n");
    out.push_str("    local def = Generated.schemas[schema_name]\n");
    out.push_str("    local values = {}\n");
    out.push_str("    if def then\n");
    out.push_str("        for i, field in ipairs(def.fields) do\n");
    out.push_str("            values[i] = OpenOxideSchema.default_value(field)\n");
    out.push_str("        end\n    end\n    return values\nend\n\n");

    out.push_str("function Generated.register(registry)\n");
    for schema in schemas {
        let _ = writeln!(
            out,
            "    OpenOxideSchema.register(registry, \"{}\", Generated.new_{})",
            escape_string(&schema.name),
            schema.name
        );
    }
    if let Some(root) = root {
        let class = root_class_name(root, schemas);
        let _ = writeln!(
            out,
            "    OpenOxideSchema.register(registry, \"{}\", Generated.new_{})",
            escape_string(&root.name),
            class
        );
    }
    out.push_str("end\n\nreturn Generated\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> (Vec<SchemaDef>, Option<RootState>) {
        let parsed = crate::ts::parse(
            r#"
            export const Player = schema({
                x: t.number(),
                y: t.number(),
                hp: t.uint8(),
                name: t.string(),
                flags: t.uint32(),
                tags: t.array(t.u32()),
                target: t.uint32().optional(),
                secret: t.uint32().transient(),
            }, "Player");
            export const Projectile = schema({
                x: t.float32(),
                owner: t.uint32(),
            }, "Projectile");
            export const State = schema({
                round: t.uint16(),
                players: t.map(Player),
                projectiles: t.map(Projectile),
            }, "State");
            "#,
        )
        .expect("sample schema parses");
        (parsed.schemas, parsed.root)
    }

    #[test]
    fn language_flags_and_file_names_are_stable() {
        assert_eq!(Language::parse("cs"), Some(Language::CSharp));
        assert_eq!(Language::parse("gdscript"), Some(Language::Gdscript));
        assert_eq!(Language::parse("c++"), Some(Language::Cpp));
        assert_eq!(Language::parse("lua"), Some(Language::Lua));
        assert_eq!(Language::parse("rust"), None);
        assert_eq!(Language::CSharp.file_name(), "OpenOxideSchemaGenerated.cs");
        assert_eq!(
            Language::Gdscript.file_name(),
            "openoxide_schema_generated.gd"
        );
        assert_eq!(Language::Cpp.file_name(), "openoxide_schema_generated.hpp");
        assert_eq!(Language::Lua.file_name(), "openoxide_schema_generated.lua");
    }

    #[test]
    fn csharp_emits_typed_entities_tables_and_registration() {
        let (schemas, root) = sample();
        let text = emit(Language::CSharp, &schemas, root.as_ref(), "schema.ts");
        assert!(text.contains("namespace OpenOxide.Schema.Generated"));
        assert!(text.contains("public sealed class Player : OxSchemaEntity"));
        assert!(text.contains("public double x = 0.0;"));
        assert!(text.contains("public byte hp = 0;"));
        assert!(text.contains("public string name = string.Empty;"));
        assert!(text.contains("public uint[] tags = new uint[0];"));
        assert!(text.contains("public uint? target = null;"));
        assert!(text.contains("new OxFieldDef(\"flags\", OxWireKind.U32, 0, OxWireKind.U8, 4)"));
        assert!(text.contains("new OxFieldDef(\"target\", OxWireKind.U32, 1, OxWireKind.U8, 4)"));
        assert!(text.contains("new OxFieldDef(\"secret\", OxWireKind.U32, 4, OxWireKind.U8, 4)"));
        assert!(text.contains("new OxSchemaCollectionDef(\"players\", \"Player\")"));
        assert!(text.contains("new OxSchemaCollectionDef(\"projectiles\", \"Projectile\")"));
        assert!(text.contains("registry.Register(\"Player\", delegate { return new Player(); });"));
        assert!(text.contains("public sealed class State : OxSchemaEntity"));
        assert!(text.contains("public ushort round = 0;"));
    }

    #[test]
    fn gdscript_emits_nested_typed_classes_and_metadata() {
        let (schemas, root) = sample();
        let text = emit(Language::Gdscript, &schemas, root.as_ref(), "schema.ts");
        assert!(text.contains("class_name OpenOxideSchemaGenerated"));
        assert!(text.contains("class Player extends OpenOxideSchema.Entity:"));
        assert!(text.contains("var x: float = 0.0"));
        assert!(text.contains("var hp: int = 0"));
        assert!(text.contains("var name: String = \"\""));
        assert!(text.contains("var tags: Array = []"));
        assert!(text.contains("var target = null"));
        assert!(text.contains("\"name\": \"flags\", \"kind\": \"u32\", \"code\": 3, \"flags\": 0, \"element\": \"none\""));
        assert!(text.contains("\"name\": \"target\", \"kind\": \"u32\", \"code\": 3, \"flags\": 1"));
        assert!(text.contains("const ROOT_COLLECTIONS := ["));
        assert!(text.contains("{\"name\": \"players\", \"schema\": \"Player\"},"));
        assert!(text.contains("registry.register(\"Player\", func(): return Player.new())"));
    }

    #[test]
    fn cpp_emits_optional_vectors_and_registration() {
        let (schemas, root) = sample();
        let text = emit(Language::Cpp, &schemas, root.as_ref(), "schema.ts");
        assert!(text.contains("#include \"openoxide_schema_runtime.hpp\""));
        assert!(text.contains("struct Player final : public Entity"));
        assert!(text.contains("double x = 0.0;"));
        assert!(text.contains("std::optional<uint32_t> target = std::nullopt;"));
        assert!(text.contains("std::vector<uint32_t> tags = {};"));
        assert!(text.contains("{\"target\", WireKind::U32, 1, WireKind::U8, 4}"));
        assert!(text.contains("registry.add(\"Player\", []() -> std::unique_ptr<Entity> { return std::make_unique<Player>(); });"));
        assert!(text.contains(
            "inline const std::vector<std::pair<std::string, std::string>>& root_collections()"
        ));
    }

    #[test]
    fn lua_emits_tables_factories_and_registration() {
        let (schemas, root) = sample();
        let text = emit(Language::Lua, &schemas, root.as_ref(), "schema.ts");
        assert!(text.contains("local OpenOxideSchema = require(\"openoxide_schema\")"));
        assert!(text.contains("Generated.schemas = {"));
        assert!(text.contains("name = \"Player\","));
        assert!(text.contains(
            "{ name = \"target\", kind = \"u32\", code = 3, flags = 1, element = nil, size = 4 },"
        ));
        assert!(text.contains("function Generated.new_Player()"));
        assert!(text.contains("return OpenOxideSchema.new_entity(Generated.schemas.Player)"));
        assert!(
            text.contains("OpenOxideSchema.register(registry, \"Player\", Generated.new_Player)")
        );
        assert!(text.contains("name = \"players\", schema = \"Player\""));
    }

    #[test]
    fn root_named_like_an_entity_gets_a_distinct_class() {
        let parsed =
            crate::ts::parse(r#"export const Player = schema({ x: t.number() }, "Player");"#)
                .unwrap();
        let schemas = parsed.schemas;
        let root = RootState {
            name: "Player".into(),
            version: 1,
            scalars: Vec::new(),
            collections: vec![crate::RootCollection {
                name: "players".into(),
                schema: "Player".into(),
            }],
        };
        assert_eq!(root_class_name(&root, &schemas), "PlayerRoot");
        let text = emit(Language::CSharp, &schemas, Some(&root), "schema.ts");
        assert!(text.contains("public sealed class Player : OxSchemaEntity"));
        assert!(text.contains("public sealed class PlayerRoot : OxSchemaEntity"));
        assert!(
            text.contains("registry.Register(\"Player\", delegate { return new PlayerRoot(); });")
        );
    }
}

// =============================================================================
// AssemblyScript server-state codegen (`--lang as`)
//
// Generates typed entity classes + a State class with per-field dirty
// tracking and `commit()` — the developer mutates fields (`p.x += 1`) and
// calls commit(); byte encoding never surfaces. Wire rows: [tag][len]
// [id u64][fields in declaration order]. Collections get tags 0x01.. in
// declaration order; the root's scalar block is one row tagged 0x10.
// =============================================================================

use crate::RootCollection;

fn as_type(kind: &WireKind) -> &'static str {
    match kind {
        WireKind::U8 => "u8",
        WireKind::I8 => "i8",
        WireKind::U16 => "u16",
        WireKind::I16 => "i16",
        WireKind::U32 => "u32",
        WireKind::I32 => "i32",
        WireKind::F32 | WireKind::F64 => "f64",
        WireKind::I64 => "i64",
        WireKind::Bool => "bool",
        WireKind::String => "string",
        _ => "f64",
    }
}

fn as_writer(kind: &WireKind) -> &'static str {
    match kind {
        WireKind::U8 => "writeU8",
        WireKind::I8 => "writeI8",
        WireKind::U16 => "writeU16",
        WireKind::I16 => "writeI16",
        WireKind::U32 => "writeU32",
        WireKind::I32 => "writeI32",
        WireKind::F32 | WireKind::F64 => "writeF64",
        WireKind::I64 => "writeI64",
        WireKind::Bool => "writeBool",
        _ => "writeF64",
    }
}

fn as_field_size(kind: &WireKind) -> usize {
    match kind {
        WireKind::U8 | WireKind::I8 | WireKind::Bool => 1,
        WireKind::U16 | WireKind::I16 => 2,
        WireKind::U32 | WireKind::I32 | WireKind::F32 => 4,
        WireKind::F64 | WireKind::I64 => 8,
        _ => 8,
    }
}

/// One entity class: typed fields + baselines + dirty() + emit().
fn emit_as_entity(out: &mut String, name: &str, fields: &[crate::SchemaField], tag_note: &str) {
    out.push_str(&format!("export class {name} {{\n"));
    let mut scalar_idx = 0usize;
    let mut field_bytes = 0usize;
    for f in fields {
        if f.flags & crate::FIELD_TRANSIENT != 0 {
            continue;
        }
        let ty = f
            .ref_target
            .as_ref()
            .map(|_| "u64".to_string()) // refs travel as entity ids
            .unwrap_or_else(|| as_type(&f.kind).to_string());
        out.push_str(&format!("    {}: {} = 0;\n", f.name, ty));
        out.push_str(&format!("    private _b{i}: {ty} = 0;\n", i = scalar_idx));
        field_bytes += if f.ref_target.is_some() { 8 } else { as_field_size(&f.kind) };
        scalar_idx += 1;
    }
    // dirty()
    out.push_str("    dirty(): bool { return ");
    if fields.is_empty() {
        out.push_str("false");
    } else {
        let checks: Vec<String> = (0..scalar_idx)
            .map(|i| format!("this.{} !== this._b{}", fields[i].name, i))
            .collect();
        out.push_str(&checks.join(" || "));
    }
    out.push_str("; }\n");
    // keep()
    out.push_str("    keep(): void { ");
    for i in 0..scalar_idx {
        out.push_str(&format!("this._b{} = this.{}; ", i, fields[i].name));
    }
    out.push_str("}\n");
    // emit()
    out.push_str(&format!(
        "    /** {tag_note} — row: [tag][len][id u64][f32 fields] */\n    emit(w: DeltaWriter, tag: u8, id: u64): void {{\n        w.beginEntity(tag, {field_bytes}, id);\n"
    ));
    for f in fields {
        if f.flags & crate::FIELD_TRANSIENT != 0 {
            continue;
        }
        if f.ref_target.is_some() {
            out.push_str(&format!("        w.writeI64(this.{} as i64);\n", f.name));
        } else {
            out.push_str(&format!(
                "        w.{}(this.{} as {});\n",
                as_writer(&f.kind),
                f.name,
                if f.kind == crate::WireKind::F64 { "f64" } else { as_type(&f.kind) }
            ));
        }
    }
    out.push_str("    }\n}\n\n");
}

pub fn emit_as(schemas: &[crate::SchemaDef], root: Option<&crate::RootState>, _source: &str) -> String {
    let mut out = String::with_capacity(4096);
    out.push_str("// GENERATED by `ox-game schema-codegen --lang as` — DO NOT EDIT.\n");
    out.push_str("// Mutate these fields in your Room; `state.commit()` emits only\n// what changed. Wire rows: [tag][len][id u64][fields].\n\n");

    let Some(root) = root else { return out };
    let schema_by_name: std::collections::HashMap<&str, &crate::SchemaDef> =
        schemas.iter().map(|s| (s.name.as_str(), s)).collect();

    // Entity classes for every referenced schema, in collection order first.
    let mut tags: Vec<(String, u8)> = Vec::new();
    let mut tag: u8 = 1;
    for c in &root.collections {
        tags.push((c.name.clone(), tag));
        tag += 1;
    }
    let mut emitted: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for c in &root.collections {
        if emitted.insert(c.schema.as_str()) {
            if let Some(def) = schema_by_name.get(c.schema.as_str()) {
                emit_as_entity(&mut out, &c.schema, &def.fields, &format!("collection '{}'", c.name));
            }
        }
    }

    // State: collections as AS Maps + root scalars as a grouped row.
    out.push_str(&format!("export class {} {{\n", root.name));
    for c in &root.collections {
        out.push_str(&format!(
            "    {}: Map<u64, {}> = new Map();\n",
            c.name, c.schema
        ));
    }
    // root scalars block
    if !root.scalars.is_empty() {
        for f in &root.scalars {
            out.push_str(&format!(
                "    {}: {} = 0;\n    private _rs{}: {} = 0;\n",
                f.name, as_type(&f.kind), 0, as_type(&f.kind)
            ));
        }
    }
    out.push_str("    private _w: DeltaWriter = new DeltaWriter(8192);\n\n");
    out.push_str("    /** Emit every dirty row; returns the packed (ptr<<32)|len delta. */\n    commit(): i64 {\n");
    for (name, t) in &tags {
        out.push_str(&format!(
            "        const ids{t} = this.{name}.keys(); const vals{t} = this.{name}.values();\n        for (let i = 0; i < ids{t}.length; i++) {{\n            const e{t} = vals{t}[i];\n            if (e{t}.dirty()) {{ e{t}.emit(this._w, {t}, ids{t}[i]); e{t}.keep(); }}\n        }}\n"
        ));
    }
    if !root.scalars.is_empty() {
        out.push_str("        { // root scalars — one row, tag 0x10\n            const w = this._w;\n");
        let mut fb = 0usize;
        for f in &root.scalars { fb += as_field_size(&f.kind); }
        out.push_str(&format!("            w.beginEntity(0x10, {fb}, 0);\n"));
        for f in &root.scalars {
            out.push_str(&format!(
                "            w.{}(this.{} as {});\n",
                as_writer(&f.kind), f.name,
                if f.kind == crate::WireKind::F64 { "f64" } else { as_type(&f.kind) }
            ));
        }
        out.push_str("        }\n");
    }
    out.push_str("        return this._w.seal();\n    }\n}\n");
    out
}

#[cfg(test)]
mod as_codegen_tests {
    use super::*;
    use crate::{RootCollection, RootState, SchemaDef, SchemaField, WireKind};

    fn field(name: &str, kind: WireKind) -> SchemaField {
        SchemaField {
            name: name.into(), kind, element: None, ref_target: None,
            flags: 0, source_type: "number".into(), line: 0, column: 0,
            default: None,
        }
    }

    #[test]
    fn as_codegen_emits_typed_state_with_dirty_commit() {
        let player = SchemaDef {
            name: "Player".into(), version: 1,
            fields: vec![field("x", WireKind::F64), field("y", WireKind::F64), field("score", WireKind::F64)],
            line: 0, column: 0,
        };
        let root = RootState {
            name: "State".into(), version: 2,
            scalars: vec![field("tick", WireKind::F64)],
            collections: vec![RootCollection { name: "players".into(), schema: "Player".into() }],
        };
        let out = emit_as(&[player], Some(&root), "schema.ts");

        // entity class: typed fields + baselines + dirty + emit row
        assert!(out.contains("export class Player {"), "entity class");
        assert!(out.contains("    x: f64 = 0;"));
        assert!(out.contains("dirty(): bool { return this.x !== this._b0 || this.y !== this._b1 || this.score !== this._b2; }"));
        assert!(out.contains("w.beginEntity(tag, 24, id);"), "player row = id(8) + 3x f64(24)");
        assert!(out.contains("w.writeF64(this.x as f64);"));

        // state: collection map + grouped root-scalar row + commit()
        assert!(out.contains("export class State {"));
        assert!(out.contains("    players: Map<u64, Player> = new Map();"));
        assert!(out.contains("e1.dirty()) { e1.emit(this._w, 1, ids1[i]); e1.keep(); }"), "dirty-only emit, tag 1");
        assert!(out.contains("w.beginEntity(0x10, 8, 0);"), "root scalars one row, tag 0x10");
        assert!(out.contains("return this._w.seal();"), "packed return");
    }
}
