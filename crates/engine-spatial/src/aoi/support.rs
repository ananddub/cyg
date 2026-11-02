use engine_config::AoiConfig;

pub(crate) fn entity_record(id: i64, pos: [f64; 3]) -> Vec<u8> {
    let mut r = Vec::with_capacity(72);
    r.extend_from_slice(&id.to_le_bytes());
    for v in pos {
        r.extend_from_slice(&v.to_le_bytes());
    }

    for _ in 0..4 {
        r.extend_from_slice(&0.0f64.to_le_bytes());
    }
    r.extend_from_slice(&1u32.to_le_bytes());
    r.extend_from_slice(&0u32.to_le_bytes());
    assert_eq!(r.len(), 72);
    r
}

pub(crate) fn bullet_record(pos: [f64; 3], owner: i64) -> Vec<u8> {
    let mut r = Vec::with_capacity(32);
    for v in pos {
        r.extend_from_slice(&v.to_le_bytes());
    }
    r.extend_from_slice(&owner.to_le_bytes());
    assert_eq!(r.len(), 32);
    r
}

pub(crate) fn synthetic_delta() -> Vec<u8> {
    let entities: [(i64, [f64; 3]); 3] = [
        (1, [0.0, 0.0, 0.0]),
        (2, [50.0, 0.0, 0.0]),
        (3, [300.0, 0.0, 0.0]),
    ];
    let bullets: [([f64; 3], i64); 2] = [([500.0, 0.0, 0.0], 1), ([1000.0, 0.0, 0.0], 2)];
    let mut d = vec![1u8];
    d.extend_from_slice(&(entities.len() as u32).to_le_bytes());
    for (id, pos) in entities {
        d.extend_from_slice(&entity_record(id, pos));
    }
    d.extend_from_slice(&(bullets.len() as u32).to_le_bytes());
    for (pos, owner) in bullets {
        d.extend_from_slice(&bullet_record(pos, owner));
    }
    d
}

pub(crate) fn test_aoi() -> AoiConfig {
    AoiConfig {
        enabled: true,
        near_radius: 100.0,
        far_radius: 400.0,
        skip_ticks: 4,
        bullet_radius: 260.0,
    }
}
