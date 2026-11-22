use crate::codegen::common::root_class_name;
use crate::codegen::{Language, emit};
use crate::{RootCollection, RootState, SchemaDef};

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
    assert_eq!(Language::CSharp.file_name(), "CYGSchemaGenerated.cs");
    assert_eq!(Language::Gdscript.file_name(), "cyg_schema_generated.gd");
    assert_eq!(Language::Cpp.file_name(), "cyg_schema_generated.hpp");
    assert_eq!(Language::Lua.file_name(), "cyg_schema_generated.lua");
}

#[test]
fn csharp_emits_typed_entities_tables_and_registration() {
    let (schemas, root) = sample();
    let text = emit(Language::CSharp, &schemas, root.as_ref(), "schema.ts");
    assert!(text.contains("namespace CYG.Schema.Generated"));
    assert!(text.contains("public sealed class Player : CygSchemaEntity"));
    assert!(text.contains("public double x = 0.0;"));
    assert!(text.contains("public byte hp = 0;"));
    assert!(text.contains("public string name = string.Empty;"));
    assert!(text.contains("public uint[] tags = new uint[0];"));
    assert!(text.contains("public uint? target = null;"));
    assert!(text.contains("new CygFieldDef(\"flags\", CygWireKind.U32, 0, CygWireKind.U8, 4)"));
    assert!(text.contains("new CygFieldDef(\"target\", CygWireKind.U32, 1, CygWireKind.U8, 4)"));
    assert!(text.contains("new CygFieldDef(\"secret\", CygWireKind.U32, 4, CygWireKind.U8, 4)"));
    assert!(text.contains("new CygSchemaCollectionDef(\"players\", \"Player\")"));
    assert!(text.contains("new CygSchemaCollectionDef(\"projectiles\", \"Projectile\")"));
    assert!(text.contains("registry.Register(\"Player\", delegate { return new Player(); });"));
    assert!(text.contains("public sealed class State : CygSchemaEntity"));
    assert!(text.contains("public ushort round = 0;"));
}

#[test]
fn gdscript_emits_nested_typed_classes_and_metadata() {
    let (schemas, root) = sample();
    let text = emit(Language::Gdscript, &schemas, root.as_ref(), "schema.ts");
    assert!(text.contains("class_name CYGSchemaGenerated"));
    assert!(text.contains("class Player extends CYGSchema.Entity:"));
    assert!(text.contains("var x: float = 0.0"));
    assert!(text.contains("var hp: int = 0"));
    assert!(text.contains("var name: String = \"\""));
    assert!(text.contains("var tags: Array = []"));
    assert!(text.contains("var target = null"));
    assert!(text.contains(
        "\"name\": \"flags\", \"kind\": \"u32\", \"code\": 3, \"flags\": 0, \"element\": \"none\""
    ));
    assert!(text.contains("\"name\": \"target\", \"kind\": \"u32\", \"code\": 3, \"flags\": 1"));
    assert!(text.contains("const ROOT_COLLECTIONS := ["));
    assert!(text.contains("{\"name\": \"players\", \"schema\": \"Player\"},"));
    assert!(text.contains("registry.register(\"Player\", func(): return Player.new())"));
}

#[test]
fn cpp_emits_optional_vectors_and_registration() {
    let (schemas, root) = sample();
    let text = emit(Language::Cpp, &schemas, root.as_ref(), "schema.ts");
    assert!(text.contains("#include \"cyg_schema_runtime.hpp\""));
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
    assert!(text.contains("local CYGSchema = require(\"cyg_schema\")"));
    assert!(text.contains("Generated.schemas = {"));
    assert!(text.contains("name = \"Player\","));
    assert!(text.contains(
        "{ name = \"target\", kind = \"u32\", code = 3, flags = 1, element = nil, size = 4 },"
    ));
    assert!(text.contains("function Generated.new_Player()"));
    assert!(text.contains("return CYGSchema.new_entity(Generated.schemas.Player)"));
    assert!(text.contains("CYGSchema.register(registry, \"Player\", Generated.new_Player)"));
    assert!(text.contains("name = \"players\", schema = \"Player\""));
}

#[test]
fn root_named_like_an_entity_gets_a_distinct_class() {
    let parsed =
        crate::ts::parse(r#"export const Player = schema({ x: t.number() }, "Player");"#).unwrap();
    let schemas = parsed.schemas;
    let root = RootState {
        nested: Vec::new(),
        name: "Player".into(),
        version: 1,
        scalars: Vec::new(),
        collections: vec![RootCollection {
            name: "players".into(),
            schema: "Player".into(),
        }],
    };
    assert_eq!(root_class_name(&root, &schemas), "PlayerRoot");
    let text = emit(Language::CSharp, &schemas, Some(&root), "schema.ts");
    assert!(text.contains("public sealed class Player : CygSchemaEntity"));
    assert!(text.contains("public sealed class PlayerRoot : CygSchemaEntity"));
    assert!(text.contains("registry.Register(\"Player\", delegate { return new PlayerRoot(); });"));
}
