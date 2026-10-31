use super::*;
use md5::Digest;

pub const STORAGE_COLUMNS: &str =
    "collection, `key`, user_id, `value`, `version`, read, write, create_time, update_time";

#[derive(Debug, Clone, Deserialize)]
pub struct StorageRow {
    pub collection: String,
    pub key: String,
    pub user_id: Uuid,
    #[serde(deserialize_with = "json_text")]
    pub value: String,
    pub version: String,
    pub read: i16,
    pub write: i16,
    #[serde(with = "time::iso")]
    pub create_time: DateTime<Utc>,
    #[serde(with = "time::iso")]
    pub update_time: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StorageCurrentRow {
    pub read: i16,
    pub write: i16,
    pub version: String,
    #[serde(with = "time::iso")]
    pub create_time: DateTime<Utc>,
    #[serde(with = "time::iso")]
    pub update_time: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StorageDeleteRow {
    pub collection: String,
    pub key: String,
    pub version: String,
    pub write: i16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StorageIndexRow {
    pub name: String,
    pub collection: String,
    pub key: String,
    pub fields: Value,
    pub sortable_fields: Value,
    pub max_entries: i32,
    pub index_only: bool,
    pub user_id: Uuid,
}

impl StorageIndexRow {
    pub fn strings(value: &Value) -> Vec<String> {
        value
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl From<StorageIndexRow> for StorageIndexDef {
    fn from(row: StorageIndexRow) -> Self {
        StorageIndexDef {
            fields: StorageIndexRow::strings(&row.fields),
            sortable_fields: StorageIndexRow::strings(&row.sortable_fields),
            collection: row.collection,
            index: row.name,
            key: row.key,
            max_entries: row.max_entries,
            index_only: row.index_only,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum StorageListing {
    PublicAll,
    Own,
    PublicUser,
}

#[derive(Debug, Clone)]
pub struct StorageCursor {
    pub key: String,
    pub read: i16,
    pub user_id: Uuid,
}

#[derive(Debug, Clone)]
pub enum StorageIndexTerm {
    Eq {
        field: String,
        text: String,
        number: Option<String>,
    },
    Range {
        field: String,
        lower: Option<StorageIndexBound>,
        upper: Option<StorageIndexBound>,
    },
}

#[derive(Debug, Clone)]
pub struct StorageIndexBound {
    pub text: String,
    pub number: Option<String>,
    pub inclusive: bool,
}

#[derive(Debug, Clone)]
pub enum StorageIndexSort {
    Column(&'static str),
    ValueField(String),
}

pub struct StorageIndexContent {
    pub name: String,
    pub collection: String,
    pub key: String,
    pub fields: Vec<String>,
    pub sortable_fields: Vec<String>,
    pub max_entries: i32,
    pub index_only: bool,
    pub user_id: Uuid,
    pub create_time: DateTime<Utc>,
}

pub fn version_of(value: &str) -> String {
    let mut hasher = md5::Md5::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn json_text<'de, D>(input: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Value::deserialize(input)?;
    Ok(match value {
        Value::String(text) => text,
        other => other.to_string(),
    })
}
