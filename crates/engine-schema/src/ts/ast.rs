use crate::{SchemaDef, WireKind};

#[derive(Clone)]
pub(super) struct Declaration {
    pub(super) schema: SchemaDef,
    pub(super) maps: Vec<(String, String)>,
    pub(super) extends: Vec<String>,
}

pub(super) struct ParsedPrimitive {
    pub(super) kind: WireKind,
    pub(super) element: Option<WireKind>,
    pub(super) ref_target: Option<String>,
    pub(super) source_type: String,
}

pub(super) enum ParsedType {
    Primitive(ParsedPrimitive),
    Map(String),
    /// A field whose type is another schema: `stats: Stats`.
    Nested(String),
    /// A field declared as `items: t.array(Item)`.
    NestedArray(String),
}

pub(super) enum Literal {
    Number(String),
    Bool(bool),
    Str(String),
}

impl Literal {
    pub(super) fn number(&self) -> Option<&str> {
        match self {
            Literal::Number(value) => Some(value),
            _ => None,
        }
    }
}
