use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("quest_definition", schemaless)]
#[index(name = "quest_definition_season", fields = "season")]
pub struct QuestDefinition {
    #[field(thing)]
    pub id: String,
    pub game_id: Option<Uuid>,
    pub title: String,
    pub goal_kind: String,
    pub goal_amount: i64,
    pub reward: serde_json::Value,
    pub season: Option<String>,
    #[serde(with = "time::iso")]
    pub active_from: DateTime<Utc>,
    #[serde(with = "time::iso")]
    pub active_to: DateTime<Utc>,
    #[serde(with = "time::iso")]
    pub create_time: DateTime<Utc>,
    #[serde(with = "time::iso")]
    pub update_time: DateTime<Utc>,
}
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("player_quest", schemaless)]
#[index(
    name = "player_quest_user_quest_uniq",
    fields = "user_id, quest_key",
    unique
)]
pub struct PlayerQuest {
    #[field(thing)]
    pub id: String,
    pub user_id: Uuid,
    pub quest_key: String,
    pub progress: i64,
    pub state: String,
    #[serde(with = "time::iso")]
    pub update_time: DateTime<Utc>,
}
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("season", schemaless)]
pub struct Season {
    #[field(thing)]
    pub id: String,
    pub game_id: Option<Uuid>,
    pub title: String,
    #[serde(with = "time::iso")]
    pub starts_at: DateTime<Utc>,
    #[serde(with = "time::iso")]
    pub ends_at: DateTime<Utc>,
    pub reward_track: serde_json::Value,
    #[serde(with = "time::iso")]
    pub create_time: DateTime<Utc>,
    #[serde(with = "time::iso")]
    pub update_time: DateTime<Utc>,
}
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("player_stat", schemaless)]
#[index(name = "player_stat_user_stat_uniq", fields = "user_id, stat", unique)]
pub struct PlayerStat {
    #[field(thing)]
    pub id: String,
    pub user_id: Uuid,
    pub stat: String,
    pub value: i64,
    #[serde(with = "time::iso")]
    pub update_time: DateTime<Utc>,
}
