use super::common::{HEADER, default_literal, emit_accessors, emit_field_builder};
use super::world::emit_world;
use crate::model::{RootState, SchemaDef};
use crate::wire::WireKind;
use crate::{INPUT_SCHEMA_NAME, JOIN_OPTIONS_SCHEMA_NAME};

pub(crate) fn emit_server(
    schemas: &[SchemaDef],
    root: Option<&RootState>,
    source_name: &str,
) -> String {
    let mut out = String::new();
    out.push_str(HEADER);
    out.push('\n');
    out.push_str(&format!(
        "// Generated from {source_name} by `cyg-game schema`. Edit the schema source, not this file.\n\n"
    ));
    let input = schemas
        .iter()
        .find(|schema| schema.name == INPUT_SCHEMA_NAME);
    let join_options = schemas
        .iter()
        .find(|schema| schema.name == JOIN_OPTIONS_SCHEMA_NAME);
    let mut imports = vec!["SchemaDef", "SchemaEntity"];
    if is_multi_root(root) {
        imports.extend(["SchemaState", "SchemaWorld"]);
    }
    if input.is_some() || join_options.is_some() {
        imports.push("InputReader");
    }
    out.push_str(&format!(
        "import {{ {} }} from \"./cyg-server\";\n",
        imports.join(", ")
    ));

    for schema in schemas {
        out.push('\n');
        out.push_str(&format!(
            "export const {}Schema = new SchemaDef(\"{}\", {})\n",
            schema.name, schema.name, schema.version
        ));
        for field in &schema.fields {
            emit_field_builder(&mut out, field);
        }

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
    if let Some(input) = input {
        emit_input_lane(&mut out, input);
    }
    if let Some(join_options) = join_options {
        emit_join_options_lane(&mut out, join_options);
    }
    out
}

fn is_multi_root(root: Option<&RootState>) -> bool {
    root.map(|root| !root.scalars.is_empty() || root.collections.len() != 1)
        .unwrap_or(false)
}

/// The reserved `Input` def becomes the per-tick input frame: this exported
/// helper decodes the client's `room.sendInput({...})` packets into the
/// generated class. Field order is the wire order — both ends derive it from
/// the same schema, so the game never hand-packs a byte.
fn emit_input_lane(out: &mut String, input: &SchemaDef) {
    out.push('\n');
    out.push_str("// --- Per-tick input lane ---------------------------------------------\n");
    out.push_str(
        "// The schema's `Input` def, decoded from the client's `room.sendInput({...})`\n",
    );
    out.push_str("// packets. Use it from the room:\n");
    out.push_str("//   onInputFrame(id: i64, r: InputReader): void {\n");
    out.push_str("//       const f = decodeInput(r);   // typed — your fields\n");
    out.push_str("//   }\n");
    out.push_str("@inline\n");
    out.push_str(&format!(
        "export function decodeInput(r: InputReader): {} {{\n    const f = new {}();\n",
        input.name, input.name
    ));
    for field in &input.fields {
        out.push_str(&format!(
            "    f.{} = r.{}();\n",
            field.name,
            input_read_method(field.kind)
        ));
    }
    out.push_str("    return f;\n}\n");

    // Allocation-free variant for the generated bridge: keeping one `Input`
    // per room and refilling it avoids an object allocation per input frame.
    out.push_str("\n/// Decode into an existing `Input` (no allocation; the bridge reuses one).\n");
    out.push_str("@inline\n");
    out.push_str(&format!(
        "export function decodeInputInto(f: {}, r: InputReader): {} {{\n",
        input.name, input.name
    ));
    for field in &input.fields {
        out.push_str(&format!(
            "    f.{} = r.{}();\n",
            field.name,
            input_read_method(field.kind)
        ));
    }
    out.push_str("    return f;\n}\n");
}

/// The InputReader cursor method matching each fixed-size field kind. String
/// and array fields never reach here — parse rejects them in an input frame.
fn input_read_method(kind: WireKind) -> &'static str {
    match kind {
        WireKind::U8 => "readU8",
        WireKind::U16 => "readU16",
        WireKind::U32 => "readU32",
        WireKind::I8 => "readI8",
        WireKind::I16 => "readI16",
        WireKind::I32 => "readI32",
        WireKind::I64 => "readI64",
        WireKind::F32 => "readF32",
        WireKind::F64 => "readF64",
        WireKind::Bool => "readBool",
        WireKind::String => "readString",
        WireKind::Array => "readF64",
    }
}

/// The reserved `JoinOptions` def types the join options: the host re-encodes
/// the client's JSON into this field order before handing the bytes to the
/// guest, so `onJoin(id, options: JoinOptions)` is fully typed — no JSON
/// parsing in the room.
fn emit_join_options_lane(out: &mut String, def: &SchemaDef) {
    out.push('\n');
    out.push_str("// --- Typed join options -----------------------------------------------\n");
    out.push_str("// The host encodes the client's joinOrCreate JSON into this field order;\n");
    out.push_str(
        "// the bridge calls onJoin(id, decodeJoinOptions(r)) — typed, no JSON parsing.\n",
    );
    out.push_str("@inline\n");
    out.push_str(&format!(
        "export function decodeJoinOptions(r: InputReader): {} {{\n    const o = new {}();\n",
        def.name, def.name
    ));
    for field in &def.fields {
        out.push_str(&format!(
            "    o.{} = r.{}();\n",
            field.name,
            input_read_method(field.kind)
        ));
    }
    out.push_str("    return o;\n}\n");
}
