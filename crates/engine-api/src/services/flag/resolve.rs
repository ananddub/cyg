use std::collections::BTreeMap;

use axum::Json;
use chrono::Utc;
use uuid::Uuid;

use super::assign::{self, Assignment};
use crate::ApiState;
use crate::dto::flag::{FlagListResponse, FlagView};
use crate::error::V2Error;
use crate::repositories::models::{FeatureFlag, FlagVariant};
use crate::services::deps::q;

impl super::FlagService {
    /// Every flag the operator defines, resolved for this one account. Nothing
    /// is cached against the account, so stopping an experiment drops it from
    /// the next reply rather than from the next login.
    pub async fn resolve(
        &self,
        state: &ApiState,
        user_id: Uuid,
    ) -> Result<Json<FlagListResponse>, V2Error> {
        let queries = q(state).await?;
        let flags = queries.list_flags().await?;
        let arms = group(queries.list_variants().await?);
        let pins = queries
            .overrides_of(user_id)
            .await?
            .into_iter()
            .map(|pin| (pin.flag_key, pin.variant))
            .collect::<BTreeMap<_, _>>();
        Ok(Json(FlagListResponse {
            flags: flags
                .iter()
                .map(|flag| {
                    let placed = decide(
                        flag,
                        arms.get(&flag.id).map_or(&[][..], Vec::as_slice),
                        pins.get(&flag.id).map(String::as_str),
                        user_id,
                    );
                    FlagView {
                        key: flag.id.clone(),
                        on: placed.on,
                        variant: placed.variant,
                    }
                })
                .collect(),
        }))
    }
}

/// Whether this account is in the arm that does something. A flag nobody
/// defines, a flag that is switched off and a flag whose arms cannot be read
/// answer the same way as an account outside the rollout: not on. Gating code
/// asks this and carries on with the plain behaviour otherwise, so an
/// unreachable database cannot turn a matchmaker into a 500.
pub async fn is_on(state: &ApiState, user_id: Uuid, flag_key: &str) -> bool {
    let Ok(queries) = q(state).await else {
        return false;
    };
    let Ok(Some(flag)) = queries.get_flag(flag_key).await else {
        return false;
    };
    if !flag.enabled {
        return false;
    }
    let arms = queries.arms_of(flag_key).await.unwrap_or_default();
    let pinned = queries
        .get_override(flag_key, user_id)
        .await
        .ok()
        .flatten()
        .map(|pin| pin.variant);
    decide(&flag, &arms, pinned.as_deref(), user_id).on
}

/// The arm one account is told about, from a flag it holds and that flag's arms,
/// which have to come back ordered by name: the first is the control. A flag
/// whose scheduled window is shut answers like one that is switched off.
pub(crate) fn decide(
    flag: &FeatureFlag,
    arms: &[FlagVariant],
    pinned: Option<&str>,
    user_id: Uuid,
) -> Assignment {
    let pairs = arms
        .iter()
        .map(|arm| (arm.name.clone(), arm.weight))
        .collect::<Vec<_>>();
    assign::place(
        &flag.id,
        &flag.salt,
        user_id,
        flag.enabled && assign::within_window(flag.active_from, flag.active_to, Utc::now()),
        flag.rollout_pct,
        &pairs,
        pinned,
    )
}

fn group(arms: Vec<FlagVariant>) -> BTreeMap<String, Vec<FlagVariant>> {
    let mut by_flag: BTreeMap<String, Vec<FlagVariant>> = BTreeMap::new();
    for arm in arms {
        by_flag.entry(arm.flag_key.clone()).or_default().push(arm);
    }
    by_flag
}
