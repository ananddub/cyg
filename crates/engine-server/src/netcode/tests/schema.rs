use crate::netcode::*;

fn schema_section(kind: u8, hash: u32, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(SCHEMA_MAGIC);
    out.push(kind);
    out.extend_from_slice(&hash.to_le_bytes());
    out.extend_from_slice(body);
    out
}

fn schema_envelope(full: &[u8], patch: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(SCHEMA_MAGIC);
    out.extend_from_slice(&(full.len() as u32).to_le_bytes());
    out.extend_from_slice(full);
    out.extend_from_slice(&(patch.len() as u32).to_le_bytes());
    out.extend_from_slice(patch);
    out
}

#[test]
fn schema_envelope_splits_full_and_patch() {
    let full = schema_section(1, 0xC737_1660, &[1, 0, 0, 0]);
    let patch = schema_section(2, 0xC737_1660, &[0, 0, 0, 0]);
    let env = schema_envelope(&full, &patch);
    let sections = split_schema(&env).expect("valid envelope");
    assert_eq!(sections.full, full.as_slice());
    assert_eq!(sections.patch, patch.as_slice());
}

#[test]
fn legacy_delta_and_truncated_schema_are_not_split() {
    assert!(split_schema(&[0, 0, 0, 0]).is_none());
    let full = schema_section(1, 7, &[0, 0, 0, 0]);
    let patch = schema_section(2, 7, &[0, 0, 0, 0]);
    let mut env = schema_envelope(&full, &patch);
    env.pop();
    assert!(
        split_schema(&env).is_none(),
        "truncated envelope must fall back"
    );
}

#[test]
fn schema_envelope_never_panics_on_malformed_input() {
    let full = schema_section(3, 0xC737_1660, &[1, 0, 0, 0]);
    let patch = schema_section(4, 0xC737_1660, &[2, 0, 0, 0]);
    let env = schema_envelope(&full, &patch);

    for end in 0..env.len() {
        assert!(
            split_schema(&env[..end]).is_none(),
            "prefix {end} must not split"
        );
    }

    let mut overflow = env.clone();
    overflow[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(split_schema(&overflow).is_none());

    let mut trailing = env.clone();
    trailing.push(0);
    assert!(split_schema(&trailing).is_none());

    let mut state = 0x9E37_79B9u32;
    for _ in 0..2_000 {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let mut mutated = env.clone();
        let position = (state as usize) % mutated.len();
        mutated[position] ^= (state >> 24) as u8 | 1;
        let _ = split_schema(&mutated);
    }

    let mut random = Vec::new();
    for length in 0..=512 {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        random.clear();
        for _ in 0..length {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            random.push((state >> 16) as u8);
        }
        let _ = split_schema(&random);
    }
}
