use super::*;

pub type TenancyGameRow = InsertGameRow;

pub type TenancyStorageRow = ListStorageRow;

#[derive(serde::Deserialize)]
pub(super) struct IdRow {
    pub(super) id: Uuid,
}

#[derive(serde::Deserialize)]
pub(super) struct CreatorRow {
    pub(super) creator_id: Uuid,
}

#[derive(serde::Deserialize)]
pub(super) struct CollectionRow {
    pub(super) collection: String,
}
