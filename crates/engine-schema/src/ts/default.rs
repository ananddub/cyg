use super::ast::Literal;
use crate::WireKind;

pub(super) fn validate_default(kind: WireKind, literal: &Literal) -> Option<String> {
    match kind {
        WireKind::Bool => match literal {
            Literal::Bool(value) => Some(value.to_string()),
            _ => None,
        },
        WireKind::String => match literal {
            Literal::Str(value) => Some(value.clone()),
            _ => None,
        },
        WireKind::F32 | WireKind::F64 => literal
            .number()?
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .map(|_| literal.number().unwrap().to_string()),
        WireKind::I64 => {
            let value = literal.number()?;
            value.parse::<i64>().ok().map(|_| value.to_string())
        }
        WireKind::U8 => range(literal, 0, u8::MAX as i64),
        WireKind::I8 => range(literal, i8::MIN as i64, i8::MAX as i64),
        WireKind::U16 => range(literal, 0, u16::MAX as i64),
        WireKind::I16 => range(literal, i16::MIN as i64, i16::MAX as i64),
        WireKind::U32 => range(literal, 0, u32::MAX as i64),
        WireKind::I32 => range(literal, i32::MIN as i64, i32::MAX as i64),
        WireKind::Array => None,
    }
}

fn range(literal: &Literal, min: i64, max: i64) -> Option<String> {
    let value = literal.number()?;
    value
        .parse::<i64>()
        .ok()
        .filter(|parsed| *parsed >= min && *parsed <= max)
        .map(|_| value.to_string())
}
