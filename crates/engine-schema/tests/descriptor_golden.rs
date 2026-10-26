//! Golden fixtures for the browser-facing schema contract.

use engine_schema::{generate_with_root, ts};

const SCHEMA_TS: &str = r#"
import { schema, t } from "@colyseus/schema";

export const Player = schema({
    x: t.number(),
    y: t.number(),
    flags: t.uint32(),
}, "Player");

export const State = schema({
    players: t.map(Player),
}, "State");
"#;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
fn descriptor_bytes_are_stable() {
    let parsed = ts::parse(SCHEMA_TS).expect("schema.ts parses");
    let generated =
        generate_with_root(&parsed.schemas, parsed.root.as_ref(), "schema.ts");
    assert_eq!(
        hex(&generated.descriptor),
        "4f585344030001000600506c6179657201000000fc46d74403000100780600000100790600000500666c6167730300000500537461746501000000bd427b23000001000700706c61796572730000",
        "descriptor layout changed; bump DESCRIPTOR_VERSION and update clients"
    );
    assert_eq!(generated.descriptor.len(), 78);
    assert_eq!(
        generated.descriptor[4], 3,
        "v3 adds per-field flags and element codes"
    );
    // Plain fields keep their historical hash; flags/element only extend it
    // when present.
    assert_eq!(generated.descriptor.windows(4).any(|w| w == [0xfc, 0x46, 0xd7, 0x44]), true);
}

#[test]
fn descriptor_v3_encodes_strings_arrays_and_flags() {
    let parsed = ts::parse(
        r#"
        const Player = schema({
            name: t.string(),
            score: t.float64().optional(),
            createdAt: t.uint32().static(),
            aim: t.array(t.float32()),
            transientField: t.uint8().transient(),
        }, "Player");
        export const State = schema({
            players: t.map(Player),
            title: t.string(),
        }, "State");
        "#,
    )
    .expect("phase 3 schema parses");
    let generated =
        generate_with_root(&parsed.schemas, parsed.root.as_ref(), "schema.ts");

    assert_eq!(
        hex(&generated.descriptor),
        "4f585344030001000600506c617965720100000003d08ce0050004006e616d650b0000050073636f72650601000900637265617465644174030200030061696d0c00050e007472616e7369656e744669656c640104000500537461746501000000bd0e90c1010005007469746c650b000001000700706c61796572730000",
        "v3 field records: name, code, flags, element"
    );

    let server = &generated.server_ts;
    assert!(server.contains(".string(\"name\")"));
    assert!(server.contains(".f64(\"score\")\n    .optional()"));
    assert!(server.contains(".u32(\"createdAt\")\n    .staticField()"));
    assert!(server.contains(".array(\"aim\", 5)"));
    assert!(server.contains(".u8(\"transientField\")\n    .transient()"));
    assert!(server.contains("hasScore(): bool"));
    assert!(server.contains("clearScore(): void"));
    assert!(server.contains("get aim(): Array<f64>"));
    assert!(server.contains("get title(): string"));

    let dts = &generated.client_dts;
    assert!(dts.contains("name: string;"));
    assert!(dts.contains("score: number | null;"));
    assert!(dts.contains("aim: Array<number>;"));
    assert!(dts.contains("title: string;"));
}

#[test]
fn generated_types_expose_generic_room_state_and_colyseus_shim() {
    let parsed = ts::parse(SCHEMA_TS).expect("schema.ts parses");
    let generated =
        generate_with_root(&parsed.schemas, parsed.root.as_ref(), "schema.ts");
    let dts = &generated.client_dts;

    assert!(dts.contains("export interface PlayerState {"));
    assert!(dts.contains("players: SchemaMap<PlayerState>;"));
    assert!(dts.contains("export type GameRoom = CYGRoom<GameState>;"));
    assert!(dts.contains("declare module \"@colyseus/schema\""));
    assert!(dts.contains("export function schema<"));
}

#[test]
fn multi_collection_root_emits_world_scalars_and_descriptor() {
    let parsed = ts::parse(
        r#"
        const Player = schema({ x: t.number(), hp: t.uint8().default(100) }, "Player");
        const Projectile = schema({ x: t.float32(), owner: t.uint32() }, "Projectile");
        export const State = schema({
            tick: t.uint32(),
            paused: t.boolean().default(false),
            players: t.map(Player),
            projectiles: t.map(Projectile),
        }, "State");
        "#,
    )
    .expect("multi-collection root parses");
    let root = parsed.root.as_ref().expect("root");
    assert_eq!(root.collections.len(), 2);
    assert_eq!(root.scalars.len(), 2);

    let generated = generate_with_root(&parsed.schemas, Some(root), "schema.ts");
    let server = &generated.server_ts;
    assert!(server.contains("export class GameState extends SchemaWorld"));
    assert!(server.contains("this.players = new SchemaState(PlayerSchema);"));
    assert!(server.contains("this.projectiles = new SchemaState(ProjectileSchema);"));
    assert!(server.contains("this.add(this.players);"));
    assert!(server.contains("get tick(): u32"));
    assert!(server.contains("this.root.setBool"));

    let dts = &generated.client_dts;
    assert!(dts.contains("tick: number;"));
    assert!(dts.contains("paused: boolean;"));
    assert!(dts.contains("players: SchemaMap<PlayerState>;"));
    assert!(dts.contains("projectiles: SchemaMap<ProjectileState>;"));

    // Root scalars and collections are declared for the browser.
    assert!(generated.descriptor.windows(9).any(|w| w == b"\x07\x00players"));
    assert!(generated.descriptor.windows(13).any(|w| w == b"\x0b\x00projectiles"));
}
