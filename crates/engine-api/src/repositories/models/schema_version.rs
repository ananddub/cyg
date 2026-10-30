use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("schema_version", schemaless)]
#[index(name = "schema_version_version_key", fields = "version", unique)]
pub struct SchemaVersion {
    #[field(thing)]
    pub id: Uuid,
    pub version: u64,
    pub name: String,
    pub checksum: String,
    #[serde(with = "time::iso")]
    pub applied_at: DateTime<Utc>,
}
