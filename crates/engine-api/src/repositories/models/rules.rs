use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("rules_definition", schemaless)]
#[index(
    name = "rules_definition_game_key_version_uniq",
    fields = "game_id, key, version",
    unique
)]
#[index(name = "rules_definition_active", fields = "game_id, key, is_active")]
pub struct RulesDefinition {
    #[field(thing)]
    pub id: String,
    pub game_id: Option<Uuid>,
    pub key: String,
    pub version: i64,
    pub sha256: String,
    pub is_active: bool,
    pub body: Value,
    #[serde(with = "time::iso")]
    pub create_time: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("game_config_version", schemaless)]
#[index(
    name = "game_config_version_game_version_uniq",
    fields = "game_id, version",
    unique
)]
pub struct GameConfigVersion {
    #[field(thing)]
    pub id: String,
    pub game_id: Uuid,
    pub version: i64,
    pub sha256: String,
    pub is_active: bool,
    pub body: Value,
    pub author: Option<String>,
    #[serde(with = "time::iso")]
    pub create_time: DateTime<Utc>,
}
