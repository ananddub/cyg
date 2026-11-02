
use engine_schema::{DESCRIPTOR_VERSION, generate_with_root, parse_descriptor, ts};

const SCHEMA_TS: &str = r#"
import { schema, t } from "@colyseus/schema";

export const Player = schema({
    x: t.number(),
    y: t.number(),
    hp: t.uint8().default(100),
    name: t.string().default("rookie"),
    tags: t.array(t.uint32()),
    target: t.uint32().optional(),
}, "Player");

export const State = schema({
    round: t.uint16().default(1),
    players: t.map(Player),
    projectiles: t.map(Player),
}, "State");
"#;

fn descriptor() -> Vec<u8> {
    let parsed = ts::parse(SCHEMA_TS).expect("schema.ts parses");
    generate_with_root(&parsed.schemas, parsed.root.as_ref(), "schema.ts").descriptor
}

struct Lcg(u32);

impl Lcg {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        self.0
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() as usize) % bound
    }
}

#[test]
fn valid_descriptor_decodes() {
    let bytes = descriptor();
    let decoded = parse_descriptor(&bytes).expect("valid descriptor decodes");
    assert_eq!(decoded.version, DESCRIPTOR_VERSION);
    assert_eq!(decoded.schemas.len(), 1);
    assert_eq!(decoded.schemas[0].name, "Player");
    assert_eq!(decoded.schemas[0].fields.len(), 6);
    assert_eq!(decoded.root.name, "State");
    assert_eq!(decoded.root.scalars.len(), 1);
    assert_eq!(decoded.collections.len(), 2);
    assert_eq!(decoded.collections[0].0, "players");
}

#[test]
fn every_truncated_prefix_is_rejected() {
    let bytes = descriptor();
    for len in 0..bytes.len() {
        assert!(
            parse_descriptor(&bytes[..len]).is_none(),
            "prefix of length {len} must not decode"
        );
    }
}

#[test]
fn trailing_bytes_are_rejected() {
    let mut bytes = descriptor();
    bytes.push(0);
    assert!(parse_descriptor(&bytes).is_none());
}

#[test]
fn newer_descriptor_version_is_rejected() {
    let mut bytes = descriptor();
    bytes[4] = DESCRIPTOR_VERSION + 1;
    assert!(parse_descriptor(&bytes).is_none());
    bytes[4] = 0;
    assert!(parse_descriptor(&bytes).is_none());
}

#[test]
fn invalid_utf8_name_is_rejected() {

    let mut bytes = descriptor();
    assert_eq!(bytes[8], 6, "first schema name is 'Player'");
    bytes[10] = 0xff;
    assert!(parse_descriptor(&bytes).is_none());
}

#[test]
fn unknown_wire_code_is_rejected() {

    let mut bytes = descriptor();
    let field_name_len = u16::from_le_bytes([bytes[26], bytes[27]]) as usize;
    assert_eq!(field_name_len, 1);
    let code_at = 28 + field_name_len;
    bytes[code_at] = 0xee;
    assert!(parse_descriptor(&bytes).is_none());
}

#[test]
fn random_buffers_never_panic() {
    let mut rng = Lcg(0x5eed_1234);
    for len in 0..=512usize {
        let mut bytes = Vec::with_capacity(len);
        for _ in 0..len {
            bytes.push(rng.next() as u8);
        }

        let _ = parse_descriptor(&bytes);
    }
}

#[test]
fn mutations_never_panic() {
    let original = descriptor();
    let mut rng = Lcg(0xdead_beef);
    for round in 0..2_000u32 {
        let mut bytes = original.clone();
        let mutations = 1 + rng.below(4);
        for _ in 0..mutations {
            let index = rng.below(bytes.len());
            let value = rng.next() as u8;

            bytes[index] = if round % 4 == 0 { 0xff } else { value };
        }
        let _ = parse_descriptor(&bytes);
    }
}
