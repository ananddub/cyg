use axum::Json;
use chrono::Utc;

use super::flag::{admin_above, percent};
use crate::ApiState;
use crate::app::middleware::ConsoleAuthed;
use crate::dto::flag::{RolloutRequest, RolloutRow};
use crate::error::V2Error;
use crate::repositories::models::FeatureFlag;
use crate::services::deps::q;
use crate::services::quest::catalog::definition_key;

impl super::ConsoleService {
    /// Moving an experiment out to a share of the accounts. Past half the
    /// population this is a rollout rather than a test, so it needs the admin.
    pub async fn set_rollout(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        raw_key: &str,
        body: RolloutRequest,
    ) -> Result<Json<RolloutRow>, V2Error> {
        claims.require_write()?;
        let pct = percent(body.percent)?;
        admin_above(claims, pct)?;
        let row = write_pct(state, raw_key, pct).await?;
        super::audit(
            state,
            claims,
            "flag.rollout",
            "flag",
            None,
            &format!("Rolled flag {} out to {pct}%.", row.id),
        )
        .await;
        Ok(Json(rolled(&row)))
    }

    /// Pulls an experiment back to nobody. The flag stays defined and keeps its
    /// salt, so restarting it puts the same accounts in the same arms.
    pub async fn stop_flag(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        raw_key: &str,
    ) -> Result<Json<RolloutRow>, V2Error> {
        claims.require_write()?;
        let row = write_pct(state, raw_key, 0).await?;
        super::audit(
            state,
            claims,
            "flag.stop",
            "flag",
            None,
            &format!("Stopped flag {}.", row.id),
        )
        .await;
        Ok(Json(rolled(&row)))
    }
}

/// The flag with its share of the population moved, everything else about the
/// experiment left exactly as it was.
async fn write_pct(state: &ApiState, raw_key: &str, pct: u32) -> Result<FeatureFlag, V2Error> {
    let key = definition_key(raw_key, "flag")?;
    let queries = q(state).await?;
    let Some(row) = queries.get_flag(&key).await? else {
        return Err(V2Error::not_found("Flag not found."));
    };
    let moved = FeatureFlag {
        id: key,
        rollout_pct: pct,
        update_time: Utc::now(),
        ..row
    };
    queries.update_flag(&moved).await?;
    Ok(moved)
}

fn rolled(row: &FeatureFlag) -> RolloutRow {
    RolloutRow {
        flag_key: row.id.clone(),
        enabled: row.enabled,
        rollout_pct: row.rollout_pct,
    }
}
