use super::*;

/// What one account has spent in one scope's current window. The row is the
/// budget's memory: a restart cannot hand a flooded account a fresh allowance,
/// and the cap it was enforced against travels with it.
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("player_quota", schemaless)]
#[index(
    name = "player_quota_user_scope_uniq",
    fields = "user_id, scope",
    unique
)]
pub struct PlayerQuota {
    #[field(thing)]
    pub id: String,
    pub user_id: Uuid,
    pub scope: String,
    pub used: u32,
    pub cap: u32,
    pub period_secs: u64,
    #[serde(with = "time::iso")]
    pub window_start: DateTime<Utc>,
    #[serde(with = "time::iso")]
    pub update_time: DateTime<Utc>,
}
