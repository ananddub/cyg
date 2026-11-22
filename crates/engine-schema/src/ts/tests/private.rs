use crate::ts::parse;
use crate::{FIELD_PRIVATE, emit_descriptor, parse_descriptor};

#[test]
fn private_marks_a_field_as_owner_only() {
    let parsed = parse(
        r#"
        const Player = schema({
            x: t.number(),
            secret: t.string().private(),
        }, "Player");
        "#,
    )
    .expect("private field parses");
    let secret = &parsed.schemas[0].fields[1];
    assert!(secret.private());
    assert_eq!(secret.flags & FIELD_PRIVATE, FIELD_PRIVATE);

    let generated = crate::emit_server(&parsed.schemas, None, "private.ts");
    assert!(generated.contains(".ownerOnly()"), "{generated}");

    let bytes = emit_descriptor(&parsed.schemas, None);
    let back = parse_descriptor(&bytes).expect("descriptor round trip");
    assert!(back.schemas[0].fields[1].private());
}
