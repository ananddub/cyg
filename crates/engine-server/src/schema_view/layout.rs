use std::collections::HashMap;

use engine_webrtc_protocol::packed::{PackedSchema, WireValue};

pub(super) struct EntityLayout {
    pub(super) packed: PackedSchema,
    pub(super) position: Option<(usize, usize, Option<usize>)>,
}

pub struct SchemaAoi {
    pub(super) by_hash: HashMap<u32, EntityLayout>,
    pub(super) root_packed: PackedSchema,
}

pub(super) struct CollView {
    pub(super) id: u16,
    pub(super) hash: u32,
    pub(super) rows: Vec<RowView>,
    pub(super) positional: bool,
}

pub(super) struct RowView {
    pub(super) id: u32,
    pub(super) pos: [f64; 3],
    pub(super) values: Vec<WireValue>,
}

pub struct FullView {
    pub(super) kind: u8,
    pub(super) root: Vec<(usize, WireValue)>,
    pub(super) collections: Vec<CollView>,
}
