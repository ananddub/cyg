use std::collections::HashMap;

use engine_schema::WireKind;

pub(super) const MAGIC: &[u8; 4] = b"OXS2";
pub(super) const SECTION_HEADER: usize = 13;
pub(super) const WORLD_HEADER: usize = 11;

#[derive(Clone, Copy)]
pub(super) struct FieldLayout {
    pub(super) kind: WireKind,
    pub(super) element: Option<WireKind>,
    pub(super) optional: bool,
}

pub(super) struct EntityLayout {
    pub(super) fields: Vec<FieldLayout>,
    pub(super) position: Option<(usize, usize, Option<usize>)>,
}

pub struct SchemaAoi {
    pub(super) by_hash: HashMap<u32, EntityLayout>,
    pub(super) root_fields: Vec<FieldLayout>,
}

pub(super) struct CollView<'a> {
    pub(super) id: u16,
    pub(super) hash: u32,
    pub(super) section: &'a [u8],
    pub(super) rows: Vec<RowView>,
    pub(super) positional: bool,
}

pub(super) struct RowView {
    pub(super) id: u32,
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) pos: [f64; 3],
}

pub struct FullView<'a> {
    pub(super) kind: u8,
    pub(super) prefix: &'a [u8],
    pub(super) collections: Vec<CollView<'a>>,
}
