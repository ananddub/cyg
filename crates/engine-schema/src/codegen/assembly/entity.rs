use crate::WireKind;

pub(super) fn as_type(kind: &WireKind) -> &'static str {
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

pub(super) fn as_writer(kind: &WireKind) -> &'static str {
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

pub(super) fn as_field_size(kind: &WireKind) -> usize {
    match kind {
        WireKind::U8 | WireKind::I8 | WireKind::Bool => 1,
        WireKind::U16 | WireKind::I16 => 2,
        WireKind::U32 | WireKind::I32 | WireKind::F32 => 4,
        WireKind::F64 | WireKind::I64 => 8,
        _ => 8,
    }
}

pub(super) fn emit_as_entity(
    out: &mut String,
    name: &str,
    fields: &[crate::SchemaField],
    tag_note: &str,
) {
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
            .map(|_| "u64".to_string())
            .unwrap_or_else(|| as_type(&f.kind).to_string());
        out.push_str(&format!("    {}: {} = 0;\n", f.name, ty));
        out.push_str(&format!("    private _b{i}: {ty} = 0;\n", i = scalar_idx));
        field_bytes += if f.ref_target.is_some() {
            8
        } else {
            as_field_size(&f.kind)
        };
        scalar_idx += 1;
    }

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

    out.push_str("    keep(): void { ");
    for i in 0..scalar_idx {
        out.push_str(&format!("this._b{} = this.{}; ", i, fields[i].name));
    }
    out.push_str("}\n");

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
                if f.kind == crate::WireKind::F64 {
                    "f64"
                } else {
                    as_type(&f.kind)
                }
            ));
        }
    }
    out.push_str("    }\n}\n\n");
}
