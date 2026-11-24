use crate::ts::parse;
use crate::{FIELD_UNRELIABLE, emit_descriptor, parse_descriptor};

#[test]
fn unreliable_marks_a_field_as_volatile() {
    let parsed = parse(
        r#"
        const Player = schema({
            x: t.f32().range(-512, 512, 20).unreliable(),
            hp: t.uint8(),
        }, "Player");
        "#,
    )
    .expect("unreliable field parses");
    let x = &parsed.schemas[0].fields[0];
    assert_eq!(x.flags & FIELD_UNRELIABLE, FIELD_UNRELIABLE);

    let generated = crate::emit_server(&parsed.schemas, None, "unreliable.ts");
    assert!(generated.contains(".unreliable()"), "{generated}");

    let bytes = emit_descriptor(&parsed.schemas, None);
    let back = parse_descriptor(&bytes).expect("descriptor round trip");
    assert_eq!(
        back.schemas[0].fields[0].flags & FIELD_UNRELIABLE,
        FIELD_UNRELIABLE
    );
}
