use crate::codegen::emit_as;
use crate::{RootCollection, RootState, SchemaDef, SchemaField, WireKind};

fn field(name: &str, kind: WireKind) -> SchemaField {
    SchemaField {
        name: name.into(),
        kind,
        element: None,
        ref_target: None,
        flags: 0,
        source_type: "number".into(),
        encoding: crate::default_encoding(kind),
        line: 0,
        column: 0,
        default: None,
    }
}

#[test]
fn as_codegen_emits_typed_state_with_dirty_commit() {
    let player = SchemaDef {
        name: "Player".into(),
        version: 1,
        fields: vec![
            field("x", WireKind::F64),
            field("y", WireKind::F64),
            field("score", WireKind::F64),
        ],
        line: 0,
        column: 0,
    };
    let root = RootState {
        name: "State".into(),
        version: 2,
        scalars: vec![field("tick", WireKind::F64)],
        collections: vec![RootCollection {
            name: "players".into(),
            schema: "Player".into(),
        }],
    };
    let out = emit_as(&[player], Some(&root), "schema.ts");

    assert!(out.contains("export class Player {"), "entity class");
    assert!(out.contains("    x: f64 = 0;"));
    assert!(out.contains("dirty(): bool { return this.x !== this._b0 || this.y !== this._b1 || this.score !== this._b2; }"));
    assert!(
        out.contains("w.beginEntity(tag, 24, id);"),
        "player row = id(8) + 3x f64(24)"
    );
    assert!(out.contains("w.writeF64(this.x as f64);"));

    assert!(out.contains("export class State extends SchemaStateBase {"));
    assert!(out.contains("    players: Map<u64, Player> = new Map();"));
    assert!(
        out.contains("e1.dirty()) { e1.emit(this._w, 1, ids1[i]); e1.keep(); }"),
        "dirty-only emit, tag 1"
    );
    assert!(
        out.contains("w.beginEntity(0x10, 8, 0);"),
        "root scalars one row, tag 0x10"
    );
    assert!(out.contains("return this._w.seal();"), "packed return");
}
