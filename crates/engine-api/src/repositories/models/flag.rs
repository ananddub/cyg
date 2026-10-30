use super::*;

/// One experiment the operator is running. `rollout_pct` is the share of
/// accounts that leaves the control arm, so `enabled = false` and
/// `rollout_pct = 0` both mean everybody is served the control — the first
/// exists to switch the whole thing off, the second to shrink it back.
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("feature_flag", schemaless)]
pub struct FeatureFlag {
    #[field(thing)]
    pub id: String,
    pub game_id: Option<Uuid>,
    pub description: String,
    pub enabled: bool,
    pub rollout_pct: u32,
    /// Changing this reshuffles who lands in which arm. A flag without its own
    /// salt is a flag that cannot be re-randomised without a second key.
    pub salt: String,
    /// The window the experiment is live for. Outside it the flag behaves
    /// exactly like a switched-off one: the control for everybody, no
    /// assignment lost, so a scheduled event opens and closes by itself.
    #[serde(with = "time::iso_opt")]
    pub active_from: Option<DateTime<Utc>>,
    #[serde(with = "time::iso_opt")]
    pub active_to: Option<DateTime<Utc>>,
    #[serde(with = "time::iso")]
    pub update_time: DateTime<Utc>,
}

/// One arm of a flag, and the share of the eligible population it takes. The
/// first arm by name is the control: what an account that is not in the rollout
/// is told to use.
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("flag_variant", schemaless)]
#[index(name = "flag_variant_flag_uniq", fields = "flag_key, name", unique)]
pub struct FlagVariant {
    #[field(thing)]
    pub id: String,
    pub flag_key: String,
    pub name: String,
    pub weight: u32,
    #[serde(with = "time::iso")]
    pub update_time: DateTime<Utc>,
}

/// The arm one account is told to use whatever the hash says. A/B is a
/// population-level statement; this is the escape hatch for one player.
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("flag_override", schemaless)]
#[index(
    name = "flag_override_flag_user_uniq",
    fields = "flag_key, user_id",
    unique
)]
pub struct FlagOverride {
    #[field(thing)]
    pub id: String,
    pub flag_key: String,
    pub user_id: Uuid,
    pub variant: String,
    #[serde(with = "time::iso")]
    pub update_time: DateTime<Utc>,
}
