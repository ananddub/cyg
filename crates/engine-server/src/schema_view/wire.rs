use engine_schema::WireKind;

use crate::schema_view::layout::FieldLayout;

pub(super) fn skip_value(buf: &[u8], mut offset: usize, field: &FieldLayout) -> Option<usize> {
    if field.optional {
        let present = *buf.get(offset)?;
        offset += 1;
        if present == 0 {
            return Some(offset);
        }
    }
    match field.kind {
        WireKind::String => {
            let len = *buf.get(offset)? as usize;
            offset += 1 + len;
            (offset <= buf.len()).then_some(offset)
        }
        WireKind::Array => {
            let count = *buf.get(offset)? as usize;
            offset += 1;
            let element = field.element?;
            if element.is_variable() {
                return None;
            }
            offset += count * element.size();
            (offset <= buf.len()).then_some(offset)
        }
        kind => {
            offset += kind.size();
            (offset <= buf.len()).then_some(offset)
        }
    }
}

pub(super) fn read_number(buf: &[u8], offset: usize, kind: WireKind) -> Option<f64> {
    let slice = |n: usize| buf.get(offset..offset + n);
    Some(match kind {
        WireKind::U8 => *slice(1)?.first()? as f64,
        WireKind::I8 => *slice(1)?.first()? as i8 as f64,
        WireKind::Bool => (*slice(1)?.first()? != 0) as u8 as f64,
        WireKind::U16 => u16::from_le_bytes(slice(2)?.try_into().ok()?) as f64,
        WireKind::I16 => i16::from_le_bytes(slice(2)?.try_into().ok()?) as f64,
        WireKind::U32 => u32::from_le_bytes(slice(4)?.try_into().ok()?) as f64,
        WireKind::I32 => i32::from_le_bytes(slice(4)?.try_into().ok()?) as f64,
        WireKind::F32 => f32::from_le_bytes(slice(4)?.try_into().ok()?) as f64,
        WireKind::F64 => f64::from_le_bytes(slice(8)?.try_into().ok()?),
        WireKind::I64 => i64::from_le_bytes(slice(8)?.try_into().ok()?) as f64,
        WireKind::String | WireKind::Array => return None,
    })
}
