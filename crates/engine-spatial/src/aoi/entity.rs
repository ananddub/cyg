#[derive(Clone, Copy, Debug)]
pub struct DeltaEntity {
    pub id: i64,
    pub pos: [f64; 3],
}

pub fn parse_delta(delta: &[u8]) -> Option<(u32, Vec<DeltaEntity>, u32, (usize, usize))> {
    if delta.len() < 5 || delta[0] != 1 {
        return None;
    }
    let count = u32::from_le_bytes(delta[1..5].try_into().ok()?) as usize;
    if delta.len() < 5 + count * 72 + 4 {
        return None;
    }
    let mut entities = Vec::with_capacity(count);
    for i in 0..count {
        let b = 5 + i * 72;
        entities.push(DeltaEntity {
            id: i64::from_le_bytes(delta[b..b + 8].try_into().ok()?),
            pos: [
                f64::from_le_bytes(delta[b + 8..b + 16].try_into().ok()?),
                f64::from_le_bytes(delta[b + 16..b + 24].try_into().ok()?),
                f64::from_le_bytes(delta[b + 24..b + 32].try_into().ok()?),
            ],
        });
    }
    let bc_off = 5 + count * 72;
    let bullets = u32::from_le_bytes(delta[bc_off..bc_off + 4].try_into().ok()?) as usize;
    let bullet_end = bc_off + 4 + bullets * 32;
    if delta.len() < bullet_end {
        return None;
    }
    Some((
        delta[0] as u32,
        entities,
        bullets as u32,
        (bc_off, bullet_end),
    ))
}

pub fn viewer_pos_of(entities: &[DeltaEntity], viewer_id: i64) -> Option<[f64; 3]> {
    entities.iter().find(|e| e.id == viewer_id).map(|e| e.pos)
}
