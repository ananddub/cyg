//! Bit-packed state sections — the v2 encoding of an OXS2 collection frame.
//!
//! A fixed-width section writes every field at its declared size: an `f32`
//! costs 4 bytes whether it holds a world position or a two-state flag. This module writes the
//! same information with the widths the schema declares ([`engine_schema::FieldEncoding`]), plus a
//! presence bit for optional fields, and keeps variable-length fields (strings, arrays) out of the
//! bit stream entirely — one misplaced bit in a packed stream would corrupt the rest of the frame,
//! so they get a byte-aligned section of their own.
//!
//! Layout (little-endian, bit stream is LSB-first inside each byte):
//!
//! ```text
//! [0..4]   "OXS2"
//! [4]      kind: 5 = full, 6 = patch
//! [5..9]   schema hash u32
//! [9..13]  op count u32
//! rows, each byte-aligned:
//!   full:   id varint, bit payload, variable payload
//!   patch:  op u8, id varint, [mask u32 for UPDATE], bit payload, variable payload
//! ```
//!
//! The bit payload length is not stored: the schema and the update mask decide it exactly, so a
//! length byte would only be a way to disagree with the decoder. Ids are LEB128, because an entity
//! id below 128 should not cost four bytes.
//!
//! Rows are byte-aligned on purpose. Concatenating rows bit-to-bit saves at most seven bits per
//! row and makes every length self-describing row depend on every row before it; a resync point
//! per row is worth more than the bits. Within a row the fields *are* packed end to end, which is
//! where the bytes are.
//!
//! Values are quantized by [`FieldEncoding`], which is the same code the client decodes with — one
//! quantizer, not two. Integer kinds use the exact path, so a `u32` keeps every bit.

use engine_schema::{FieldEncoding, SchemaDef, WireKind, wire_hash};

use crate::bits::{BitReader, BitWriter};

/// `OXS2` section magic shared by every packed section.
pub const MAGIC: &[u8; 4] = b"OXS2";
pub const KIND_FULL: u8 = 5;
pub const KIND_PATCH: u8 = 6;
/// A whole world: root scalars plus every collection, each in its own packed
/// section. The guest emits these directly; fixed-width world kinds 3/4 are
/// rejected and require a room wasm rebuild.
pub const KIND_WORLD_FULL: u8 = 7;
pub const KIND_WORLD_PATCH: u8 = 8;
pub const HEADER_LEN: usize = 13;
/// `magic + kind + root hash + root field count`.
pub const WORLD_HEADER_LEN: usize = 11;
/// `collection id + schema hash + section length`.
pub const WORLD_COLLECTION_HEADER_LEN: usize = 10;

pub const OP_ADD: u8 = 1;
pub const OP_REMOVE: u8 = 2;
pub const OP_UPDATE: u8 = 3;

/// One field value. Mirrors the OXS2 `Value` enum; arrays keep their element kind.
#[derive(Clone, Debug, PartialEq)]
pub enum WireValue {
    U8(u8),
    U16(u16),
    U32(u32),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
    Bool(bool),
    Str(String),
    Arr(Vec<WireValue>),
    /// An absent optional field: one bit on the wire, nothing else.
    Null,
}

impl WireValue {
    fn as_f32(&self) -> f32 {
        match *self {
            WireValue::U8(v) => v as f32,
            WireValue::U16(v) => v as f32,
            WireValue::U32(v) => v as f32,
            WireValue::I8(v) => v as f32,
            WireValue::I16(v) => v as f32,
            WireValue::I32(v) => v as f32,
            WireValue::I64(v) => v as f32,
            WireValue::F32(v) => v,
            WireValue::F64(v) => v as f32,
            WireValue::Bool(v) => u8::from(v) as f32,
            WireValue::Null | WireValue::Str(_) | WireValue::Arr(_) => 0.0,
        }
    }

    fn as_i64(&self) -> i64 {
        match *self {
            WireValue::U8(v) => v as i64,
            WireValue::U16(v) => v as i64,
            WireValue::U32(v) => v as i64,
            WireValue::I8(v) => v as i64,
            WireValue::I16(v) => v as i64,
            WireValue::I32(v) => v as i64,
            WireValue::I64(v) => v,
            WireValue::Bool(v) => i64::from(v),
            WireValue::F32(v) => v as i64,
            WireValue::F64(v) => v as i64,
            WireValue::Null | WireValue::Str(_) | WireValue::Arr(_) => 0,
        }
    }
}

/// How one schema field travels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Slot {
    /// Quantized into bits: integers, bools, and floats with a declared range.
    Bits(FieldEncoding),
    /// Byte-aligned in the variable section: raw floats, `i64`, strings, arrays.
    Raw(WireKind),
}

/// A schema compiled for the packed lane. Build once per descriptor, reuse per tick.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackedSchema {
    hash: u32,
    fields: Vec<Slot>,
    kinds: Vec<WireKind>,
    optional: Vec<bool>,
    element: Vec<Option<WireKind>>,
}

impl PackedSchema {
    pub fn from_def(schema: &SchemaDef) -> Self {
        let fields = schema
            .fields
            .iter()
            .map(|field| {
                let enc = field.encoding;
                // A string or array never packs; neither does a float without a declared range,
                // nor a 64-bit integer, whose value does not survive the quantizer.
                let packable = enc.is_packed()
                    && !matches!(
                        field.kind,
                        WireKind::String | WireKind::Array | WireKind::I64
                    );
                if packable {
                    Slot::Bits(enc)
                } else {
                    Slot::Raw(field.kind)
                }
            })
            .collect();
        Self {
            hash: wire_hash(schema),
            kinds: schema.fields.iter().map(|f| f.kind).collect(),
            fields,
            optional: schema.fields.iter().map(|f| f.optional()).collect(),
            element: schema.fields.iter().map(|f| f.element).collect(),
        }
    }

    pub fn hash(&self) -> u32 {
        self.hash
    }

    pub fn field_count(&self) -> usize {
        self.fields.len()
    }

    /// Bits a full row of this schema costs, ignoring the variable section.
    pub fn row_bits(&self) -> usize {
        self.fields
            .iter()
            .zip(self.optional.iter())
            .map(|(slot, optional)| {
                let presence = usize::from(*optional);
                presence
                    + match slot {
                        Slot::Bits(enc) => enc.bits as usize,
                        Slot::Raw(_) => 0,
                    }
            })
            .sum()
    }
}

/// One row of a patch.
#[derive(Clone, Debug, PartialEq)]
pub enum PackedOp {
    Add {
        id: u32,
        values: Vec<WireValue>,
    },
    Remove {
        id: u32,
    },
    Update {
        id: u32,
        mask: u32,
        values: Vec<(usize, WireValue)>,
    },
}

/// A decoded packed section.
#[derive(Clone, Debug, PartialEq)]
pub struct Decoded {
    pub kind: u8,
    pub ops: Vec<PackedOp>,
    /// Values the encoder had to clamp into their declared range — a non-zero count means the
    /// schema's range is too small for the game (see Gaffer: check, do not trust).
    pub clamped: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PackedError {
    Truncated,
    BadMagic,
    UnsupportedKind(u8),
    HashMismatch { found: u32, expected: u32 },
    FieldCount { row: usize, got: usize, want: usize },
    IntegerRange { field: usize, value: i64 },
    TooManyFields(usize),
}

impl std::fmt::Display for PackedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Truncated => write!(f, "truncated packed section"),
            Self::BadMagic => write!(f, "packed section magic mismatch"),
            Self::UnsupportedKind(kind) => write!(f, "unknown packed section kind {kind}"),
            Self::HashMismatch { found, expected } => write!(
                f,
                "packed schema hash mismatch: frame {found:08x}, local {expected:08x}"
            ),
            Self::FieldCount { row, got, want } => write!(
                f,
                "packed row {row} carries {got} values, schema declares {want}"
            ),
            Self::IntegerRange { field, value } => {
                write!(f, "field {field} integer {value} does not fit its encoding")
            }
            Self::TooManyFields(fields) => {
                write!(f, "{fields} fields do not fit in a 32-bit update mask")
            }
        }
    }
}

impl std::error::Error for PackedError {}

/// True when `bytes` starts a packed section (collection or world). Lets a decoder branch without
/// parsing the whole frame.
pub fn is_packed_section(bytes: &[u8]) -> bool {
    bytes.len() > 4
        && &bytes[0..4] == MAGIC
        && matches!(
            bytes[4],
            KIND_FULL | KIND_PATCH | KIND_WORLD_FULL | KIND_WORLD_PATCH
        )
}

/// A decoded packed world frame.
#[derive(Clone, Debug, PartialEq)]
pub struct DecodedWorld {
    pub kind: u8,
    /// Fields the frame carries. A full frame carries every root field; a patch
    /// carries only the bits set here.
    pub root_mask: u32,
    pub root: Vec<(usize, WireValue)>,
    /// `(collection id, schema hash, packed collection section)` in wire order.
    pub collections: Vec<(u16, u32, Vec<u8>)>,
}

fn all_field_bits(count: usize) -> u32 {
    if count >= 32 {
        u32::MAX
    } else {
        (1u32 << count) - 1
    }
}

/// Encodes a full world: every root scalar, then one packed full section per
/// collection. Root scalars use their own synthetic schema hash, exactly like
/// the guest's world header.
pub fn encode_world_full(
    root: &PackedSchema,
    values: &[WireValue],
    collections: &[(u16, u32, Vec<u8>)],
    clamped: &mut u32,
) -> Result<Vec<u8>, PackedError> {
    if root.fields.len() > 32 {
        return Err(PackedError::TooManyFields(root.fields.len()));
    }
    if values.len() != root.fields.len() {
        return Err(PackedError::FieldCount {
            row: 0,
            got: values.len(),
            want: root.fields.len(),
        });
    }
    let mut out = world_header(root, KIND_WORLD_FULL);
    write_row(root, values.iter().enumerate(), &mut out, clamped)?;
    write_collections(&mut out, collections)?;
    Ok(out)
}

/// Encodes a world patch: masked root scalars, then only the collections that
/// changed, each as a packed patch section.
pub fn encode_world_patch(
    root: &PackedSchema,
    root_mask: u32,
    values: &[(usize, WireValue)],
    collections: &[(u16, u32, Vec<u8>)],
    clamped: &mut u32,
) -> Result<Vec<u8>, PackedError> {
    if root.fields.len() > 32 {
        return Err(PackedError::TooManyFields(root.fields.len()));
    }
    let mut out = world_header(root, KIND_WORLD_PATCH);
    out.extend_from_slice(&root_mask.to_le_bytes());
    write_row(
        root,
        values
            .iter()
            .filter(|(index, _)| *index < 32 && root_mask & (1 << *index) != 0)
            .map(|(index, value)| (*index, value)),
        &mut out,
        clamped,
    )?;
    write_collections(&mut out, collections)?;
    Ok(out)
}

fn world_header(root: &PackedSchema, kind: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(WORLD_HEADER_LEN + 8);
    out.extend_from_slice(MAGIC);
    out.push(kind);
    out.extend_from_slice(&root.hash.to_le_bytes());
    out.extend_from_slice(&(root.fields.len() as u16).to_le_bytes());
    out
}

fn write_collections(
    out: &mut Vec<u8>,
    collections: &[(u16, u32, Vec<u8>)],
) -> Result<(), PackedError> {
    if collections.len() > u16::MAX as usize {
        return Err(PackedError::TooManyFields(collections.len()));
    }
    out.extend_from_slice(&(collections.len() as u16).to_le_bytes());
    for (id, hash, bytes) in collections {
        let len = u32::try_from(bytes.len()).map_err(|_| PackedError::Truncated)?;
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(&hash.to_le_bytes());
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(bytes);
    }
    Ok(())
}

/// Decodes a packed world frame. Collection sections are returned verbatim:
/// the caller resolves each schema hash and decodes them with [`decode`].
pub fn decode_world(root: &PackedSchema, bytes: &[u8]) -> Result<DecodedWorld, PackedError> {
    if bytes.len() < WORLD_HEADER_LEN {
        return Err(PackedError::Truncated);
    }
    if &bytes[0..4] != MAGIC {
        return Err(PackedError::BadMagic);
    }
    let kind = bytes[4];
    if !matches!(kind, KIND_WORLD_FULL | KIND_WORLD_PATCH) {
        return Err(PackedError::UnsupportedKind(kind));
    }
    let found = u32::from_le_bytes(bytes[5..9].try_into().map_err(|_| PackedError::Truncated)?);
    if found != root.hash {
        return Err(PackedError::HashMismatch {
            found,
            expected: root.hash,
        });
    }
    let count = u16::from_le_bytes(
        bytes[9..11]
            .try_into()
            .map_err(|_| PackedError::Truncated)?,
    ) as usize;
    if count != root.fields.len() {
        return Err(PackedError::FieldCount {
            row: 0,
            got: count,
            want: root.fields.len(),
        });
    }
    let mut at = WORLD_HEADER_LEN;
    let root_mask = if kind == KIND_WORLD_PATCH {
        let mask = u32::from_le_bytes(
            bytes
                .get(at..at + 4)
                .ok_or(PackedError::Truncated)?
                .try_into()
                .unwrap(),
        );
        at += 4;
        mask
    } else {
        all_field_bits(root.fields.len())
    };
    let root_values = read_row(root, bytes, &mut at, 0, true, root_mask)?;

    let collections = read_collections(bytes, &mut at)?;
    if at != bytes.len() {
        return Err(PackedError::Truncated);
    }
    Ok(DecodedWorld {
        kind,
        root_mask,
        root: root_values,
        collections,
    })
}

fn read_collections(bytes: &[u8], at: &mut usize) -> Result<Vec<(u16, u32, Vec<u8>)>, PackedError> {
    let count = u16::from_le_bytes(
        bytes
            .get(*at..*at + 2)
            .ok_or(PackedError::Truncated)?
            .try_into()
            .unwrap(),
    ) as usize;
    *at += 2;
    if count > bytes.len() {
        return Err(PackedError::Truncated);
    }
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let header = bytes
            .get(*at..*at + WORLD_COLLECTION_HEADER_LEN)
            .ok_or(PackedError::Truncated)?;
        let id = u16::from_le_bytes(header[0..2].try_into().unwrap());
        let hash = u32::from_le_bytes(header[2..6].try_into().unwrap());
        let len = u32::from_le_bytes(header[6..10].try_into().unwrap()) as usize;
        *at += WORLD_COLLECTION_HEADER_LEN;
        let section = bytes
            .get(*at..*at + len)
            .ok_or(PackedError::Truncated)?
            .to_vec();
        *at += len;
        out.push((id, hash, section));
    }
    Ok(out)
}

/// Encodes a full snapshot: every row in full.
pub fn encode_full(
    schema: &PackedSchema,
    rows: &[(u32, Vec<WireValue>)],
    clamped: &mut u32,
) -> Result<Vec<u8>, PackedError> {
    let mut out = header(schema, KIND_FULL, rows.len())?;
    for (row, (id, values)) in rows.iter().enumerate() {
        if values.len() != schema.fields.len() {
            return Err(PackedError::FieldCount {
                row,
                got: values.len(),
                want: schema.fields.len(),
            });
        }
        write_varint(&mut out, *id);
        write_row(
            schema,
            values.iter().enumerate().map(|(i, v)| (i, v)),
            &mut out,
            clamped,
        )?;
    }
    Ok(out)
}

/// Encodes a patch: adds, removes and masked updates.
pub fn encode_patch(
    schema: &PackedSchema,
    ops: &[PackedOp],
    clamped: &mut u32,
) -> Result<Vec<u8>, PackedError> {
    let mut out = header(schema, KIND_PATCH, ops.len())?;
    for (row, op) in ops.iter().enumerate() {
        match op {
            PackedOp::Add { id, values } => {
                if values.len() != schema.fields.len() {
                    return Err(PackedError::FieldCount {
                        row,
                        got: values.len(),
                        want: schema.fields.len(),
                    });
                }
                out.push(OP_ADD);
                write_varint(&mut out, *id);
                write_row(
                    schema,
                    values.iter().enumerate().map(|(i, v)| (i, v)),
                    &mut out,
                    clamped,
                )?;
            }
            PackedOp::Remove { id } => {
                out.push(OP_REMOVE);
                write_varint(&mut out, *id);
            }
            PackedOp::Update { id, mask, values } => {
                out.push(OP_UPDATE);
                write_varint(&mut out, *id);
                out.extend_from_slice(&mask.to_le_bytes());
                write_row(
                    schema,
                    values.iter().map(|(i, v)| (*i, v)),
                    &mut out,
                    clamped,
                )?;
            }
        }
    }
    Ok(out)
}

/// LEB128, the same shape protobuf uses for ids: one byte below 128, more when needed.
fn write_varint(out: &mut Vec<u8>, mut value: u32) {
    loop {
        let byte = (value & 0x7F) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

fn read_varint(bytes: &[u8], at: &mut usize) -> Result<u32, PackedError> {
    let mut value = 0u32;
    let mut shift = 0;
    loop {
        let byte = *bytes.get(*at).ok_or(PackedError::Truncated)?;
        *at += 1;
        value |= ((byte & 0x7F) as u32) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
        shift += 7;
        if shift >= 35 {
            return Err(PackedError::Truncated);
        }
    }
}

fn header(schema: &PackedSchema, kind: u8, count: usize) -> Result<Vec<u8>, PackedError> {
    if schema.fields.len() > 32 {
        return Err(PackedError::TooManyFields(schema.fields.len()));
    }
    let mut out = Vec::with_capacity(HEADER_LEN + count * 8);
    out.extend_from_slice(MAGIC);
    out.push(kind);
    out.extend_from_slice(&schema.hash.to_le_bytes());
    out.extend_from_slice(&(count as u32).to_le_bytes());
    Ok(out)
}

/// Writes `[bit_len u16][bit payload][variable payload]`.
fn write_row<'v>(
    schema: &PackedSchema,
    values: impl Iterator<Item = (usize, &'v WireValue)>,
    out: &mut Vec<u8>,
    clamped: &mut u32,
) -> Result<(), PackedError> {
    let mut bits = BitWriter::new();
    let mut vars: Vec<u8> = Vec::new();
    for (index, value) in values {
        let optional = schema.optional.get(index).copied().unwrap_or(false);
        let absent = matches!(value, WireValue::Null);
        if optional {
            bits.write_bool(!absent);
            if absent {
                continue;
            }
        }
        match schema.fields[index] {
            Slot::Bits(enc) => {
                if enc.integer {
                    let integer = value.as_i64();
                    let (floor, ceiling) = (
                        enc.min().round() as i64,
                        enc.min().round() as i64 + ((1u64 << enc.bits) - 1) as i64,
                    );
                    if integer < floor || integer > ceiling {
                        *clamped += 1;
                    }
                    bits.write_bits(enc.quantize_int(integer), enc.bits);
                } else {
                    let float = value.as_f32();
                    if !enc.in_range(float) {
                        *clamped += 1;
                    }
                    bits.write_bits(enc.quantize(float), enc.bits);
                }
            }
            Slot::Raw(kind) => write_variable(&mut vars, kind, value, schema.element[index])?,
        }
    }
    let payload = bits.finish();
    out.extend_from_slice(payload);
    out.extend_from_slice(&vars);
    Ok(())
}

/// Variable payloads keep the byte-aligned shape: `len u8 + utf8`, `count u8 + elements`,
/// little-endian scalars. No reason to invent a second encoding for them.
fn write_variable(
    out: &mut Vec<u8>,
    kind: WireKind,
    value: &WireValue,
    element: Option<WireKind>,
) -> Result<(), PackedError> {
    match kind {
        WireKind::String => {
            let text = match value {
                WireValue::Str(text) => text.as_str(),
                _ => "",
            };
            let bytes = &text.as_bytes()[..text.len().min(u8::MAX as usize)];
            out.push(bytes.len() as u8);
            out.extend_from_slice(bytes);
        }
        WireKind::Array => {
            let items: &[WireValue] = match value {
                WireValue::Arr(items) => items,
                _ => &[],
            };
            let count = items.len().min(u8::MAX as usize);
            out.push(count as u8);
            let kind = element.unwrap_or(WireKind::F64);
            for item in &items[..count] {
                write_element(out, kind, item)?;
            }
        }
        WireKind::F32 => out.extend_from_slice(&value.as_f32().to_le_bytes()),
        WireKind::F64 => out.extend_from_slice(&as_f64(value).to_le_bytes()),
        WireKind::I64 => out.extend_from_slice(&value.as_i64().to_le_bytes()),
        // Everything else is packed; reaching here means the slot and the value disagree.
        _ => {}
    }
    Ok(())
}

fn as_f64(value: &WireValue) -> f64 {
    match value {
        WireValue::F64(v) => *v,
        WireValue::F32(v) => *v as f64,
        other => other.as_f32() as f64,
    }
}

fn write_element(out: &mut Vec<u8>, kind: WireKind, value: &WireValue) -> Result<(), PackedError> {
    match kind {
        WireKind::U8 => out.push(match value {
            WireValue::U8(v) => *v,
            other => other.as_i64() as u8,
        }),
        WireKind::I8 => out.push(match value {
            WireValue::I8(v) => *v as u8,
            other => other.as_i64() as i8 as u8,
        }),
        WireKind::U16 => out.extend_from_slice(&(value.as_i64() as u16).to_le_bytes()),
        WireKind::I16 => out.extend_from_slice(&(value.as_i64() as i16).to_le_bytes()),
        WireKind::U32 => out.extend_from_slice(&(value.as_i64() as u32).to_le_bytes()),
        WireKind::I32 => out.extend_from_slice(&(value.as_i64() as i32).to_le_bytes()),
        WireKind::F32 => out.extend_from_slice(&value.as_f32().to_le_bytes()),
        WireKind::F64 => out.extend_from_slice(&as_f64(value).to_le_bytes()),
        WireKind::Bool => out.push(u8::from(value.as_i64() != 0)),
        WireKind::I64 => out.extend_from_slice(&value.as_i64().to_le_bytes()),
        WireKind::String | WireKind::Array => {}
    }
    Ok(())
}

/// Decodes a packed section. Every read is bounds-checked; a short or corrupt frame is an error,
/// never a partial row.
pub fn decode(schema: &PackedSchema, bytes: &[u8]) -> Result<Decoded, PackedError> {
    if bytes.len() < HEADER_LEN {
        return Err(PackedError::Truncated);
    }
    if &bytes[0..4] != MAGIC {
        return Err(PackedError::BadMagic);
    }
    let kind = bytes[4];
    if !matches!(kind, KIND_FULL | KIND_PATCH) {
        return Err(PackedError::UnsupportedKind(kind));
    }
    let found = u32::from_le_bytes(bytes[5..9].try_into().map_err(|_| PackedError::Truncated)?);
    if found != schema.hash {
        return Err(PackedError::HashMismatch {
            found,
            expected: schema.hash,
        });
    }
    let count = u32::from_le_bytes(
        bytes[9..13]
            .try_into()
            .map_err(|_| PackedError::Truncated)?,
    ) as usize;
    // A corrupt count must not turn into a huge allocation: every row costs at least 6 bytes, so a
    // count larger than the frame is simply wrong.
    if count > bytes.len() {
        return Err(PackedError::Truncated);
    }
    let mut at = HEADER_LEN;
    let mut ops = Vec::with_capacity(count);

    for row in 0..count {
        let op = if kind == KIND_FULL {
            OP_ADD
        } else {
            let op = *bytes.get(at).ok_or(PackedError::Truncated)?;
            at += 1;
            op
        };
        let id = read_varint(bytes, &mut at)?;
        if op == OP_REMOVE {
            ops.push(PackedOp::Remove { id });
            continue;
        }
        let mask = if op == OP_UPDATE {
            let mask = u32::from_le_bytes(
                bytes
                    .get(at..at + 4)
                    .ok_or(PackedError::Truncated)?
                    .try_into()
                    .unwrap(),
            );
            at += 4;
            mask
        } else {
            0
        };

        let values = read_row(schema, bytes, &mut at, row, op == OP_UPDATE, mask)?;
        match op {
            OP_ADD => {
                let mut row_values: Vec<WireValue> = vec![WireValue::Null; schema.fields.len()];
                for (index, value) in values {
                    row_values[index] = value;
                }
                ops.push(PackedOp::Add {
                    id,
                    values: row_values,
                });
            }
            OP_UPDATE => ops.push(PackedOp::Update { id, mask, values }),
            other => return Err(PackedError::UnsupportedKind(other)),
        }
    }
    if at != bytes.len() {
        return Err(PackedError::Truncated);
    }
    Ok(Decoded {
        kind,
        ops,
        clamped: 0,
    })
}

fn read_row(
    schema: &PackedSchema,
    bytes: &[u8],
    at: &mut usize,
    row: usize,
    masked: bool,
    mask: u32,
) -> Result<Vec<(usize, WireValue)>, PackedError> {
    let payload = bytes.get(*at..).ok_or(PackedError::Truncated)?;
    let mut bits = BitReader::new(payload);
    let mut bit_values = Vec::with_capacity(schema.fields.len());
    let mut raw_fields = Vec::new();
    let mut values = Vec::with_capacity(schema.fields.len());

    for index in 0..schema.fields.len() {
        if masked && (mask & (1 << index)) == 0 {
            continue;
        }
        if schema.optional[index] {
            let present = bits.read_bool().ok_or(PackedError::Truncated)?;
            if !present {
                values.push((index, WireValue::Null));
                continue;
            }
        }
        match schema.fields[index] {
            Slot::Bits(enc) => {
                let code = bits.read_bits(enc.bits).ok_or(PackedError::Truncated)?;
                bit_values.push((index, decode_bits(enc, schema.kinds[index], code, row)?));
            }
            Slot::Raw(_) => raw_fields.push(index),
        }
    }
    let bit_bytes = bits.bits_read().div_ceil(8);
    let mut vars = at.checked_add(bit_bytes).ok_or(PackedError::Truncated)?;
    values.extend(bit_values);
    for index in raw_fields {
        let Slot::Raw(kind) = schema.fields[index] else {
            unreachable!()
        };
        values.push((
            index,
            read_variable(bytes, &mut vars, kind, schema.element[index])?,
        ));
    }
    values.sort_by_key(|(index, _)| *index);
    // The next row starts immediately after this row's padded bit bytes and variables.
    *at = vars;
    Ok(values)
}

/// Restores the schema's own variant: the bit stream carries a code, the kind decides how to read
/// it. Decoding a `u8` field as an `i64` would hand the client a value it can never compare equal.
fn decode_bits(
    enc: FieldEncoding,
    kind: WireKind,
    code: u64,
    row: usize,
) -> Result<WireValue, PackedError> {
    if !enc.integer {
        return Ok(match kind {
            WireKind::F32 => WireValue::F32(enc.dequantize(code)),
            _ => WireValue::F64(enc.dequantize(code) as f64),
        });
    }
    let integer = enc.dequantize_int(code);
    Ok(match kind {
        WireKind::Bool => WireValue::Bool(integer != 0),
        WireKind::U8 => WireValue::U8(integer as u8),
        WireKind::I8 => WireValue::I8(integer as i8),
        WireKind::U16 => WireValue::U16(integer as u16),
        WireKind::I16 => WireValue::I16(integer as i16),
        WireKind::U32 => WireValue::U32(integer as u32),
        WireKind::I32 => WireValue::I32(integer as i32),
        WireKind::I64 => WireValue::I64(integer),
        _ => {
            return Err(PackedError::IntegerRange {
                field: row,
                value: integer,
            });
        }
    })
}

fn read_variable(
    bytes: &[u8],
    at: &mut usize,
    kind: WireKind,
    element: Option<WireKind>,
) -> Result<WireValue, PackedError> {
    Ok(match kind {
        WireKind::String => {
            let len = *bytes.get(*at).ok_or(PackedError::Truncated)? as usize;
            *at += 1;
            let slice = bytes.get(*at..*at + len).ok_or(PackedError::Truncated)?;
            *at += len;
            WireValue::Str(String::from_utf8_lossy(slice).into_owned())
        }
        WireKind::Array => {
            let count = *bytes.get(*at).ok_or(PackedError::Truncated)? as usize;
            *at += 1;
            let kind = element.unwrap_or(WireKind::F64);
            let mut items = Vec::with_capacity(count.min(bytes.len()));
            for _ in 0..count {
                items.push(read_element(bytes, at, kind)?);
            }
            WireValue::Arr(items)
        }
        WireKind::F32 => {
            let v = read_array(bytes, at, 4)?;
            WireValue::F32(f32::from_bits(v as u32))
        }
        WireKind::F64 => {
            let v = read_array(bytes, at, 8)?;
            WireValue::F64(f64::from_bits(v))
        }
        WireKind::I64 => WireValue::I64(read_array(bytes, at, 8)? as i64),
        _ => return Err(PackedError::Truncated),
    })
}

fn read_element(bytes: &[u8], at: &mut usize, kind: WireKind) -> Result<WireValue, PackedError> {
    let width = kind.size();
    let raw = read_array(bytes, at, width)?;
    Ok(match kind {
        WireKind::U8 => WireValue::U8(raw as u8),
        WireKind::I8 => WireValue::I8(raw as i8),
        WireKind::U16 => WireValue::U16(raw as u16),
        WireKind::I16 => WireValue::I16(raw as i16),
        WireKind::U32 => WireValue::U32(raw as u32),
        WireKind::I32 => WireValue::I32(raw as i32),
        WireKind::F32 => WireValue::F32(f32::from_bits(raw as u32)),
        WireKind::F64 => WireValue::F64(f64::from_bits(raw)),
        WireKind::Bool => WireValue::Bool(raw != 0),
        WireKind::I64 => WireValue::I64(raw as i64),
        WireKind::String | WireKind::Array => WireValue::Bool(false),
    })
}

fn read_array(bytes: &[u8], at: &mut usize, width: usize) -> Result<u64, PackedError> {
    let slice = bytes.get(*at..*at + width).ok_or(PackedError::Truncated)?;
    *at += width;
    let mut value = 0u64;
    for (i, byte) in slice.iter().enumerate() {
        value |= (*byte as u64) << (8 * i);
    }
    Ok(value)
}

#[cfg(test)]
mod tests;
