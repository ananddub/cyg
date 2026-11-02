use crate::ts::parse;
use crate::WireKind;

#[test]
fn rejects_unsupported_containers_and_field_version() {
    let errors = parse(r#"const Bad = schema({ samples: t.array(t.int64()) }, "Bad");"#).unwrap_err();
    assert!(errors.iter().any(|e| e.message.contains("loses precision")));

    let errors = parse(r#"const Bad = schema({ names: t.array(t.string()) }, "Bad");"#).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("numeric or boolean"))
    );

    let errors = parse(r#"const Bad = schema({ items: t.set(t.number()) }, "Bad");"#).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("collections are not supported"))
    );

    let errors = parse(r#"const Bad = schema({ hp: t.uint8().version(2) }, "Bad");"#).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("applies to the whole schema"))
    );
}

#[test]
fn rejects_out_of_range_defaults_and_duplicate_fields() {
    let errors = parse(r#"const Bad = schema({ hp: t.uint8().default(300) }, "Bad");"#).unwrap_err();
    assert!(errors.iter().any(|e| e.message.contains("out of range")));

    let errors = parse(r#"const Bad = schema({ x: t.number(), x: t.number() }, "Bad");"#).unwrap_err();
    assert!(errors.iter().any(|e| e.message.contains("more than once")));
}

#[test]
fn parses_multi_collection_root_with_scalars() {
    let source = r#"
        const Player = schema({ x: t.number() }, "Player");
        const Projectile = schema({ speed: t.float32() }, "Projectile");
        export const State = schema({
            tick: t.uint32(),
            paused: t.boolean(),
            players: t.map(Player),
            projectiles: t.map(Projectile),
        }, "State");
    "#;
    let parsed = parse(source).expect("multi root parses");
    assert_eq!(
        parsed.schemas.len(),
        2,
        "root scalars are not an entity schema"
    );
    let root = parsed.root.expect("root world");
    assert_eq!(root.name, "State");
    assert_eq!(root.scalars.len(), 2);
    assert_eq!(root.scalars[0].kind, WireKind::U32);
    assert_eq!(root.scalars[1].kind, WireKind::Bool);
    assert_eq!(root.collections.len(), 2);
    assert_eq!(root.collections[0].name, "players");
    assert_eq!(root.collections[0].schema, "Player");
    assert_eq!(root.collections[1].name, "projectiles");
}

#[test]
fn rejects_collection_conflicts_and_unknown_targets() {
    let duplicate = parse(
        r#"
        const P = schema({ x: t.number() }, "P");
        const S = schema({ players: t.map(P), players: t.map(P) }, "S");
        "#,
    )
    .unwrap_err();
    assert!(
        duplicate
            .iter()
            .any(|e| e.message.contains("more than once"))
    );

    let unknown = parse(
        r#"
        const P = schema({ x: t.number() }, "P");
        const S = schema({ ghosts: t.map(Missing) }, "S");
        "#,
    )
    .unwrap_err();
    assert!(
        unknown
            .iter()
            .any(|e| e.message.contains("unknown schema 'Missing'"))
    );

    let shadow = parse(
        r#"
        const P = schema({ x: t.number() }, "P");
        const S = schema({ size: t.uint32(), p: t.map(P) }, "S");
        "#,
    )
    .unwrap_err();
    assert!(shadow.iter().any(|e| e.message.contains("Map member")));
}
