use crate::codegen::{Language, emit};

fn sample() -> (Vec<crate::SchemaDef>, Option<crate::RootState>) {
    let parsed = crate::ts::parse(
        r#"
        export const Player = schema({
            x: t.number(),
            hp: t.uint8(),
            name: t.string(),
            tags: t.array(t.u32()),
            target: t.uint32().optional(),
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
fn typescript_flag_and_file_name_are_stable() {
    assert_eq!(Language::parse("ts"), Some(Language::TypeScript));
    assert_eq!(Language::parse("typescript"), Some(Language::TypeScript));
    assert_eq!(
        Language::TypeScript.file_name(),
        "cyg_schema_generated.d.ts"
    );
    assert_eq!(Language::TypeScript.flag(), "ts");
}

#[test]
fn typescript_emits_typed_interfaces_and_runtime_tables() {
    let (schemas, root) = sample();
    let text = emit(Language::TypeScript, &schemas, root.as_ref(), "schema.ts");

    // Typed row interfaces with primitive narrowing.
    assert!(text.contains("export interface PlayerState {"));
    assert!(text.contains("    x: number;"));
    assert!(text.contains("    hp: number;"));
    assert!(text.contains("    name: string;"));
    assert!(text.contains("    tags: readonly number[];"));
    assert!(text.contains("    target: number | null;"));

    // Runtime schema tables (kind metadata) with wire hash.
    assert!(text.contains("export const PlayerSchema: CygSchemaTableDef = {"));
    assert!(text.contains(
        "{ name: \"hp\", kind: \"u8\", code: 1, flags: 0, element: \"none\", size: 1 },"
    ));
    assert!(text.contains("import type { Room, SchemaMap } from \"cyg\";"));

    // Root state with typed collections and a typed room alias.
    assert!(text.contains("export interface StateState {"));
    assert!(text.contains("    round: number;"));
    assert!(text.contains("    players: SchemaMap<PlayerState>;"));
    assert!(text.contains("    projectiles: SchemaMap<ProjectileState>;"));
    assert!(text.contains("export type StateRoom = Room<StateState>;"));
    assert!(text.contains("export const rootCollections = ["));
    assert!(text.contains("{ name: \"players\", schema: \"Player\" },"));

    // Registry + window global.
    assert!(text.contains("export const schemas = {"));
    assert!(text.contains("    Player: PlayerSchema,"));
    assert!(text.contains("interface Window {\n        CYGSchema: typeof schemas;"));
}
