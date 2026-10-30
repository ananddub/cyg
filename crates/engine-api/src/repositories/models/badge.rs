use super::*;

/// What one counter has to reach for a player to hold a badge. The threshold is
/// compared against `player_stat` every time a badge list is asked for, so an
/// operator re-tuning a threshold re-judges every player at once and no player
/// row has to be rewritten.
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("badge_definition", schemaless)]
#[index(name = "badge_definition_stat_idx", fields = "stat")]
pub struct BadgeDefinition {
    #[field(thing)]
    pub id: String,
    pub game_id: Option<Uuid>,
    pub title: String,
    pub detail: String,
    /// The counter this badge reads, named the way an event names it.
    pub stat: String,
    pub threshold: i64,
    pub is_active: bool,
    #[serde(with = "time::iso")]
    pub create_time: DateTime<Utc>,
    #[serde(with = "time::iso")]
    pub update_time: DateTime<Utc>,
}
