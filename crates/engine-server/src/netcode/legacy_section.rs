//! Reads the guest's legacy (fixed-width) OXS2 section back into values.
//!
//! State is produced inside the guest wasm, so the host normally only ever sees bytes. The packed
//! lane needs the values: it quantizes them, so it cannot work on the fixed-width bytes directly.
//! This is the read half of that round trip — the same layout `cyg.js` and `cyg-core-rs` decode,
//! ported once here so the host can transcode without a second schema language.
//!
//! This is the **guest ABI**, not the client wire. Both collection sections (1/2) and world
//! sections (3/4) are read here; [`PackedSet`](super::PackedSet) re-encodes every one of them into
//! the packed kinds 5-8 that clients receive. There is no fixed-width client path.

use engine_schema::{SchemaDef, SchemaField, WireKind};

use engine_webrtc_protocol::packed::{PackedOp, WireValue};

pub const MAGIC: &[u8; 4] = b"OXS2";
/// Guest-side collection kinds (fixed-width). These exist only between the room
/// wasm and the host now; clients receive the packed kinds 5-8.
pub const KIND_FULL: u8 = 1;
pub const KIND_PATCH: u8 = 2;
pub const KIND_WORLD_FULL: u8 = 3;
pub const KIND_WORLD_PATCH: u8 = 4;

const OP_ADD: u8 = 1;
const OP_REMOVE: u8 = 2;
const OP_UPDATE: u8 = 3;

/// A section header, already validated against the schema it claims to describe.
pub struct Header {
    pub kind: u8,
    pub count: usize,
    pub at: usize,
}

fn get(bytes: &[u8], at: usize) -> Result<u8, String> {
    bytes
        .get(at)
        .copied()
        .ok_or_else(|| format!("truncated section at {at} (len {})", bytes.len()))
}

fn u16_at(bytes: &[u8], at: &mut usize) -> Result<u16, String> {
    let value = u16::from_le_bytes([get(bytes, *at)?, get(bytes, *at + 1)?]);
    *at += 2;
    Ok(value)
}

fn u32_at(bytes: &[u8], at: &mut usize) -> Result<u32, String> {
    let value = u32::from_le_bytes([
        get(bytes, *at)?,
        get(bytes, *at + 1)?,
        get(bytes, *at + 2)?,
        get(bytes, *at + 3)?,
    ]);
    *at += 4;
    Ok(value)
}

fn u64_at(bytes: &[u8], at: &mut usize) -> Result<u64, String> {
    let low = u32_at(bytes, at)? as u64;
    let high = u32_at(bytes, at)? as u64;
    Ok(low | (high << 32))
}

/// Validates the magic, the kind and the schema hash, and returns where the rows start.
pub fn header(section: &[u8], schema: &SchemaDef) -> Result<Header, String> {
    if section.len() < 13 {
        return Err("truncated section".into());
    }
    if &section[0..4] != MAGIC {
        return Err("section magic mismatch".into());
    }
    let kind = section[4];
    if !matches!(kind, KIND_FULL | KIND_PATCH) {
        return Err(format!("section kind {kind} is not a collection section"));
    }
    let mut at = 5;
    let hash = u32_at(section, &mut at)?;
    let expected = engine_schema::wire_hash(schema);
    if hash != expected {
        return Err(format!(
            "schema hash mismatch: section {hash:08x}, descriptor {expected:08x}"
        ));
    }
    let count = u32_at(section, &mut at)? as usize;
    // A corrupt count must not become an allocation or a long loop.
    if count > section.len() {
        return Err("section row count exceeds its length".into());
    }
    Ok(Header { kind, count, at })
}

/// Decodes a full section into the rows it carries, in wire order.
pub fn decode_full(section: &[u8], schema: &SchemaDef) -> Result<Vec<(u32, Vec<WireValue>)>, String> {
    let header = header(section, schema)?;
    if header.kind != KIND_FULL {
        return Err("expected a full section".into());
    }
    let mut at = header.at;
    let mut rows = Vec::with_capacity(header.count);
    for _ in 0..header.count {
        let id = u32_at(section, &mut at)?;
        let values = read_values(section, &mut at, schema)?;
        rows.push((id, values));
    }
    if at != section.len() {
        return Err("trailing bytes after the last row".into());
    }
    Ok(rows)
}

/// Decodes a patch section into the same op list the packed encoder takes.
pub fn decode_patch(section: &[u8], schema: &SchemaDef) -> Result<Vec<PackedOp>, String> {
    let header = header(section, schema)?;
    if header.kind != KIND_PATCH {
        return Err("expected a patch section".into());
    }
    let mut at = header.at;
    let mut ops = Vec::with_capacity(header.count);
    for _ in 0..header.count {
        let op = get(section, at)?;
        at += 1;
        match op {
            OP_ADD => {
                let id = u32_at(section, &mut at)?;
                let values = read_values(section, &mut at, schema)?;
                ops.push(PackedOp::Add { id, values });
            }
            OP_REMOVE => {
                let id = u32_at(section, &mut at)?;
                ops.push(PackedOp::Remove { id });
            }
            OP_UPDATE => {
                let id = u32_at(section, &mut at)?;
                let mask = u32_at(section, &mut at)?;
                let mut values = Vec::new();
                for (index, field) in schema.fields.iter().enumerate() {
                    if mask & (1 << index) == 0 {
                        continue;
                    }
                    values.push((index, read_value(section, &mut at, field)?));
                }
                ops.push(PackedOp::Update { id, mask, values });
            }
            other => return Err(format!("unknown patch op {other}")),
        }
    }
    if at != section.len() {
        return Err("trailing bytes after the last op".into());
    }
    Ok(ops)
}

/// A collection section carried inside a guest world frame.
pub struct WorldCollection<'a> {
    pub id: u16,
    pub hash: u32,
    pub section: &'a [u8],
}

/// Decoded guest world full (kind 3).
pub struct WorldFull<'a> {
    pub root: Vec<WireValue>,
    pub collections: Vec<WorldCollection<'a>>,
}

/// Decoded guest world patch (kind 4): dirty root fields plus changed collections.
pub struct WorldPatch<'a> {
    pub root: Vec<(usize, WireValue)>,
    pub root_mask: u32,
    pub collections: Vec<WorldCollection<'a>>,
}

fn world_header(section: &[u8], root_schema: &SchemaDef, kind: u8) -> Result<(usize, u32), String> {
    if section.len() < 11 {
        return Err("truncated world section".into());
    }
    if &section[0..4] != MAGIC {
        return Err("section magic mismatch".into());
    }
    if section[4] != kind {
        return Err(format!("expected world kind {kind}, got {}", section[4]));
    }
    let mut at = 5;
    let hash = u32_at(section, &mut at)?;
    let expected = engine_schema::wire_hash(root_schema);
    if hash != expected {
        return Err(format!(
            "root schema hash mismatch: section {hash:08x}, descriptor {expected:08x}"
        ));
    }
    let count = u16_at(section, &mut at)? as usize;
    if count != root_schema.fields.len() {
        return Err(format!(
            "root field count mismatch: section {count}, descriptor {}",
            root_schema.fields.len()
        ));
    }
    Ok((at, hash))
}

/// Decodes a guest world full section into root values plus nested sections.
pub fn decode_world_full<'a>(
    section: &'a [u8],
    root_schema: &SchemaDef,
) -> Result<WorldFull<'a>, String> {
    let (mut at, _) = world_header(section, root_schema, KIND_WORLD_FULL)?;
    let mut root = Vec::with_capacity(root_schema.fields.len());
    for field in &root_schema.fields {
        root.push(read_value(section, &mut at, field)?);
    }
    let collections = read_collections(section, &mut at)?;
    if at != section.len() {
        return Err("trailing bytes in schema world section".into());
    }
    Ok(WorldFull { root, collections })
}

/// Decodes a guest world patch section.
pub fn decode_world_patch<'a>(
    section: &'a [u8],
    root_schema: &SchemaDef,
) -> Result<WorldPatch<'a>, String> {
    let (mut at, _) = world_header(section, root_schema, KIND_WORLD_PATCH)?;
    let mask = u32_at(section, &mut at)?;
    let mut root = Vec::new();
    for (index, field) in root_schema.fields.iter().enumerate() {
        if mask & (1 << index) == 0 {
            continue;
        }
        root.push((index, read_value(section, &mut at, field)?));
    }
    let collections = read_collections(section, &mut at)?;
    if at != section.len() {
        return Err("trailing bytes in schema world patch".into());
    }
    Ok(WorldPatch {
        root,
        root_mask: mask,
        collections,
    })
}

fn read_collections<'a>(
    section: &'a [u8],
    at: &mut usize,
) -> Result<Vec<WorldCollection<'a>>, String> {
    let count = u16_at(section, at)? as usize;
    if count > section.len() {
        return Err("collection count exceeds its length".into());
    }
    let mut collections = Vec::with_capacity(count);
    for _ in 0..count {
        let id = u16_at(section, at)?;
        let hash = u32_at(section, at)?;
        let len = u32_at(section, at)? as usize;
        let end = at.checked_add(len).ok_or("collection length overflow")?;
        let body = section
            .get(*at..end)
            .ok_or("truncated collection section")?;
        *at = end;
        collections.push(WorldCollection {
            id,
            hash,
            section: body,
        });
    }
    Ok(collections)
}

fn read_values(section: &[u8], at: &mut usize, schema: &SchemaDef) -> Result<Vec<WireValue>, String> {
    let mut values = Vec::with_capacity(schema.fields.len());
    for field in &schema.fields {
        values.push(read_value(section, at, field)?);
    }
    Ok(values)
}

/// One field: optional presence byte, then the kind's fixed-width or length-prefixed form.
fn read_value(section: &[u8], at: &mut usize, field: &SchemaField) -> Result<WireValue, String> {
    if field.optional() {
        let present = get(section, *at)?;
        *at += 1;
        if present == 0 {
            return Ok(WireValue::Null);
        }
    }
    Ok(match field.kind {
        WireKind::String => {
            let len = get(section, *at)? as usize;
            *at += 1;
            let bytes = section
                .get(*at..*at + len)
                .ok_or("truncated string")?;
            *at += len;
            WireValue::Str(String::from_utf8_lossy(bytes).into_owned())
        }
        WireKind::Array => {
            let count = get(section, *at)? as usize;
            *at += 1;
            let element = field.element.unwrap_or(WireKind::F64);
            let mut items = Vec::with_capacity(count.min(section.len()));
            for _ in 0..count {
                items.push(read_element(section, at, element)?);
            }
            WireValue::Arr(items)
        }
        WireKind::U8 => {
            let value = get(section, *at)?;
            *at += 1;
            WireValue::U8(value)
        }
        WireKind::I8 => {
            let value = get(section, *at)? as i8;
            *at += 1;
            WireValue::I8(value)
        }
        WireKind::Bool => {
            let value = get(section, *at)? != 0;
            *at += 1;
            WireValue::Bool(value)
        }
        WireKind::U16 => WireValue::U16(u16_at(section, at)?),
        WireKind::I16 => WireValue::I16(u16_at(section, at)? as i16),
        WireKind::U32 => WireValue::U32(u32_at(section, at)?),
        WireKind::I32 => WireValue::I32(u32_at(section, at)? as i32),
        WireKind::F32 => WireValue::F32(f32::from_bits(u32_at(section, at)?)),
        WireKind::F64 => WireValue::F64(f64::from_bits(u64_at(section, at)?)),
        WireKind::I64 => WireValue::I64(u64_at(section, at)? as i64),
    })
}

fn read_element(section: &[u8], at: &mut usize, kind: WireKind) -> Result<WireValue, String> {
    Ok(match kind {
        WireKind::U8 => {
            let value = get(section, *at)?;
            *at += 1;
            WireValue::U8(value)
        }
        WireKind::I8 => {
            let value = get(section, *at)? as i8;
            *at += 1;
            WireValue::I8(value)
        }
        WireKind::Bool => {
            let value = get(section, *at)? != 0;
            *at += 1;
            WireValue::Bool(value)
        }
        WireKind::U16 => WireValue::U16(u16_at(section, at)?),
        WireKind::I16 => WireValue::I16(u16_at(section, at)? as i16),
        WireKind::U32 => WireValue::U32(u32_at(section, at)?),
        WireKind::I32 => WireValue::I32(u32_at(section, at)? as i32),
        WireKind::F32 => WireValue::F32(f32::from_bits(u32_at(section, at)?)),
        WireKind::F64 => WireValue::F64(f64::from_bits(u64_at(section, at)?)),
        WireKind::I64 => WireValue::I64(u64_at(section, at)? as i64),
        WireKind::String | WireKind::Array => {
            return Err("array elements cannot themselves be variable length".into());
        }
    })
}

#[cfg(test)]
mod tests;
