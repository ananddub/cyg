use crate::ts::parse;

#[test]
fn supports_inheritance_third_arg_and_extend_chain() {
    let source = r#"
        const Base = schema({ x: t.number(), hp: t.uint8() }, "Base");
        const Child = schema({ name: t.string() }, "Child", Base);
        const Grand = schema({ score: t.uint32() }, "Grand").extend(Child).version(3);
    "#;
    let parsed = parse(source).expect("inheritance parses");
    let names: Vec<&str> = parsed.schemas[2]
        .fields
        .iter()
        .map(|field| field.name.as_str())
        .collect();
    assert_eq!(
        names,
        ["x", "hp", "name", "score"],
        "base fields come first"
    );
    assert_eq!(parsed.schemas[2].version, 3);
    assert_eq!(parsed.schemas[1].version, 1);
}

#[test]
fn rejects_bad_inheritance_and_ref_targets() {
    let errors = parse(
        r#"
        const Base = schema({ x: t.number() }, "Base");
        const Child = schema({ x: t.uint8() }, "Child").extend(Base);
        "#,
    )
    .unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("already inherited"))
    );

    let errors =
        parse(r#"const Child = schema({ x: t.number() }, "Child").extend(Missing);"#).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("extends unknown schema"))
    );

    let errors = parse(r#"const Bad = schema({ target: t.ref(Missing) }, "Bad");"#).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("references unknown schema"))
    );

    let cycle = parse(
        r#"
        const A = schema({ a: t.number() }, "A").extend(B);
        const B = schema({ b: t.number() }, "B").extend(A);
        "#,
    )
    .unwrap_err();
    assert!(cycle.iter().any(|e| e.message.contains("cycle")));
}
