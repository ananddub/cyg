use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::repositories::models::{FeatureFlag, FlagOverride, FlagVariant};

/// The player's view of one experiment: which arm they were placed in and
/// whether that arm is the one that does something.
#[derive(Debug, Serialize)]
pub struct FlagView {
    pub key: String,
    pub on: bool,
    pub variant: String,
}

#[derive(Debug, Serialize)]
pub struct FlagListResponse {
    pub flags: Vec<FlagView>,
}

/// What the operator typed to create or rewrite a flag. `salt` is optional:
/// leaving it out gives the flag a fresh one, and setting it deliberately is the
/// only way to reshuffle who sits in which arm.
#[derive(Debug, Deserialize)]
pub struct FeatureFlagRequest {
    #[serde(default)]
    pub flag_key: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub rollout_pct: Option<u32>,
    #[serde(default)]
    pub salt: Option<String>,
    /// The window the experiment is live for. Absent keeps the window that is
    /// written and a timestamp sets it, so switching a flag off does not quietly
    /// reschedule it; a window that has closed behaves like a flag that is off.
    #[serde(default)]
    pub active_from: Option<DateTime<Utc>>,
    #[serde(default)]
    pub active_to: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct FeatureFlagRow {
    pub flag_key: String,
    pub description: String,
    pub enabled: bool,
    pub rollout_pct: u32,
    pub salt: String,
    pub active_from: Option<DateTime<Utc>>,
    pub active_to: Option<DateTime<Utc>>,
    pub update_time: DateTime<Utc>,
}

impl From<FeatureFlag> for FeatureFlagRow {
    fn from(row: FeatureFlag) -> Self {
        Self {
            flag_key: row.id,
            description: row.description,
            enabled: row.enabled,
            rollout_pct: row.rollout_pct,
            salt: row.salt,
            active_from: row.active_from,
            active_to: row.active_to,
            update_time: row.update_time,
        }
    }
}

/// Moving an experiment out to a share of the population. Above half the
/// population this needs the admin role: past that point it is a rollout, not a
/// test.
#[derive(Debug, Deserialize)]
pub struct RolloutRequest {
    pub percent: u32,
}

#[derive(Debug, Serialize)]
pub struct RolloutRow {
    pub flag_key: String,
    pub enabled: bool,
    pub rollout_pct: u32,
}

/// One arm of a flag and the share of the eligible population it takes. Weights
/// are relative, so `60/40` and `6/4` and `3/2` all describe the same split.
#[derive(Debug, Deserialize)]
pub struct VariantRequest {
    #[serde(default)]
    pub name: Option<String>,
    pub weight: u32,
}

#[derive(Debug, Serialize)]
pub struct VariantRow {
    pub flag_key: String,
    pub name: String,
    pub weight: u32,
    pub is_control: bool,
}

impl VariantRow {
    /// The control is the arm an account that is not in the rollout gets, which
    /// is the first arm by name. The caller holds the flag's arms in that order.
    pub fn new(variant: FlagVariant, control: bool) -> Self {
        Self {
            flag_key: variant.flag_key,
            name: variant.name,
            weight: variant.weight,
            is_control: control,
        }
    }
}

/// Pinning one account to one arm whatever the hash would have said.
#[derive(Debug, Deserialize)]
pub struct OverrideRequest {
    pub user_id: String,
    pub variant: String,
}

#[derive(Debug, Serialize)]
pub struct OverrideRow {
    pub flag_key: String,
    pub user_id: String,
    pub variant: String,
}

impl From<FlagOverride> for OverrideRow {
    fn from(row: FlagOverride) -> Self {
        Self {
            flag_key: row.flag_key,
            user_id: row.user_id.to_string(),
            variant: row.variant,
        }
    }
}
