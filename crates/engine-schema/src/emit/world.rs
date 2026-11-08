use super::common::{capitalize, default_literal, emit_field_builder};
use crate::model::RootState;
use crate::wire::{WireKind, array_assemblyscript_type};

pub(super) fn emit_world(out: &mut String, root: &RootState) {
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
            WireKind::Array => array_assemblyscript_type(field.element.unwrap_or(WireKind::F64)),
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
