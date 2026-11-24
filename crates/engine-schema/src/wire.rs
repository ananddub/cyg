#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireKind {
    U8,
    I8,
    U16,
    I16,
    U32,
    I32,
    F32,
    F64,
    Bool,
    I64,

    String,

    Array,
}

pub const FIELD_OPTIONAL: u8 = 1;

pub const FIELD_STATIC: u8 = 2;

pub const FIELD_TRANSIENT: u8 = 4;

/// Owner-only field: the host blanks it for every row whose id is not the
/// viewer's own id. The descriptor still carries the field so patch shapes
/// stay stable; the value never reaches another client.
pub const FIELD_PRIVATE: u8 = 8;

/// Volatile field: sent on the unreliable state stream only. The reliable
/// state stream omits it, so a dropped volatile frame never holds up a
/// must-deliver field (inventory, score, quest state).
pub const FIELD_UNRELIABLE: u8 = 16;

impl WireKind {
    pub fn size(self) -> usize {
        match self {
            WireKind::U8 | WireKind::I8 | WireKind::Bool => 1,
            WireKind::U16 | WireKind::I16 => 2,
            WireKind::U32 | WireKind::I32 | WireKind::F32 => 4,
            WireKind::F64 | WireKind::I64 => 8,
            WireKind::String | WireKind::Array => 0,
        }
    }

    pub fn is_variable(self) -> bool {
        matches!(self, WireKind::String | WireKind::Array)
    }

    pub fn from_code(code: u8) -> Option<WireKind> {
        Some(match code {
            1 => WireKind::U8,
            2 => WireKind::U16,
            3 => WireKind::U32,
            4 => WireKind::I32,
            5 => WireKind::F32,
            6 => WireKind::F64,
            7 => WireKind::Bool,
            8 => WireKind::I64,
            9 => WireKind::I8,
            10 => WireKind::I16,
            11 => WireKind::String,
            12 => WireKind::Array,
            _ => return None,
        })
    }

    pub fn code(self) -> u8 {
        match self {
            WireKind::U8 => 1,
            WireKind::U16 => 2,
            WireKind::U32 => 3,
            WireKind::I32 => 4,
            WireKind::F32 => 5,
            WireKind::F64 => 6,
            WireKind::Bool => 7,
            WireKind::I64 => 8,
            WireKind::I8 => 9,
            WireKind::I16 => 10,
            WireKind::String => 11,
            WireKind::Array => 12,
        }
    }

    pub fn builder_method(self) -> &'static str {
        match self {
            WireKind::U8 => "u8",
            WireKind::I8 => "i8",
            WireKind::U16 => "u16",
            WireKind::I16 => "i16",
            WireKind::U32 => "u32",
            WireKind::I32 => "i32",
            WireKind::F32 => "f32",
            WireKind::F64 => "f64",
            WireKind::Bool => "bool",
            WireKind::I64 => "i64",
            WireKind::String => "string",
            WireKind::Array => "array",
        }
    }

    pub fn assemblyscript_type(self) -> &'static str {
        match self {
            WireKind::U8 => "u8",
            WireKind::I8 => "i8",
            WireKind::U16 => "u16",
            WireKind::I16 => "i16",
            WireKind::U32 => "u32",
            WireKind::I32 => "i32",
            WireKind::F32 => "f32",
            WireKind::F64 => "f64",
            WireKind::Bool => "bool",
            WireKind::I64 => "i64",
            WireKind::String => "string",

            WireKind::Array => "Array<f64>",
        }
    }

    pub fn typescript_type(self) -> &'static str {
        match self {
            WireKind::Bool => "boolean",
            WireKind::String => "string",
            WireKind::Array => "Array<number>",
            _ => "number",
        }
    }

    pub(crate) fn value_suffix(self) -> &'static str {
        match self {
            WireKind::U8 => "U8",
            WireKind::I8 => "I8",
            WireKind::U16 => "U16",
            WireKind::I16 => "I16",
            WireKind::U32 => "U32",
            WireKind::I32 => "I32",
            WireKind::F32 => "F32",
            WireKind::F64 => "F64",
            WireKind::Bool => "Bool",
            WireKind::I64 => "I64",
            WireKind::String => "String",
            WireKind::Array => "Array",
        }
    }
}

pub fn array_assemblyscript_type(_element: WireKind) -> &'static str {
    "Array<f64>"
}

pub fn array_typescript_type(_element: WireKind) -> &'static str {
    "Array<number>"
}
