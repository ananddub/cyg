use axum::Json;
use chrono::Utc;
use uuid::Uuid;

use crate::ApiState;
use crate::app::middleware::ConsoleAuthed;
use crate::dto::Empty;
use crate::dto::flag::{FeatureFlagRequest, FeatureFlagRow};
use crate::error::V2Error;
use crate::repositories::models::FeatureFlag;
use crate::services::deps::q;
use crate::services::quest::catalog::definition_key;

/// Above this share of the population an experiment is a rollout, not a test,
/// so it needs the admin role rather than any account that can write.
pub(super) const ADMIN_ABOVE: u32 = 50;

impl super::ConsoleService {
    pub async fn list_flags(&self, state: &ApiState) -> Result<Json<Vec<FeatureFlagRow>>, V2Error> {
        let rows = q(state).await?.list_flags().await?;
        Ok(Json(rows.into_iter().map(FeatureFlagRow::from).collect()))
    }

    pub async fn create_flag(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        body: FeatureFlagRequest,
    ) -> Result<Json<FeatureFlagRow>, V2Error> {
        claims.require_write()?;
        let key = definition_key(&body.flag_key, "flag")?;
        let pct = percent(body.rollout_pct.unwrap_or(0))?;
        admin_above(claims, pct)?;
        let row = FeatureFlag {
            id: key.clone(),
            game_id: crate::services::games::tenant::game_id(state).await?,
            description: described(&body.description, "")?,
            enabled: body.enabled.unwrap_or(true),
            rollout_pct: pct,
            salt: salt(body.salt.as_deref(), None)?,
            active_from: body.active_from,
            active_to: body.active_to,
            update_time: Utc::now(),
        };
        let row = scheduled(row)?;
        if !q(state).await?.create_flag(&row).await? {
            return Err(V2Error::already_exists(
                "A flag with this key already exists.",
            ));
        }
        super::audit(
            state,
            claims,
            "flag.create",
            "flag",
            None,
            &format!("Created flag {key} at {pct}%."),
        )
        .await;
        Ok(Json(FeatureFlagRow::from(row)))
    }

    /// Rewrites what the operator left out rather than resetting it: a body
    /// without a `salt` keeps the salt, because a new salt would move every
    /// account to a different arm of a test that is still measuring people.
    pub async fn update_flag(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        raw_key: &str,
        body: FeatureFlagRequest,
    ) -> Result<Json<FeatureFlagRow>, V2Error> {
        claims.require_write()?;
        let key = definition_key(raw_key, "flag")?;
        let queries = q(state).await?;
        let Some(existing) = queries.get_flag(&key).await? else {
            return Err(V2Error::not_found("Flag not found."));
        };
        let pct = body
            .rollout_pct
            .map(percent)
            .transpose()?
            .unwrap_or(existing.rollout_pct);
        admin_above(claims, pct)?;
        let row = FeatureFlag {
            id: key.clone(),
            game_id: existing.game_id,
            description: described(&body.description, &existing.description)?,
            enabled: body.enabled.unwrap_or(existing.enabled),
            rollout_pct: pct,
            salt: salt(body.salt.as_deref(), Some(&existing.salt))?,
            active_from: body.active_from.or(existing.active_from),
            active_to: body.active_to.or(existing.active_to),
            update_time: Utc::now(),
        };
        let row = scheduled(row)?;
        queries.update_flag(&row).await?;
        super::audit(
            state,
            claims,
            "flag.update",
            "flag",
            None,
            &format!("Wrote flag {key}."),
        )
        .await;
        Ok(Json(FeatureFlagRow::from(row)))
    }

    /// Drops the flag and every arm and exemption defined against it, so a
    /// deleted experiment cannot keep assigning a key nothing defines anymore.
    pub async fn delete_flag(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        raw_key: &str,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_write()?;
        let key = definition_key(raw_key, "flag")?;
        let queries = q(state).await?;
        if queries.delete_flag(&key).await?.rows_affected() == 0 {
            return Err(V2Error::not_found("Flag not found."));
        }
        queries.delete_flag_parts(&key).await?;
        super::audit(
            state,
            claims,
            "flag.delete",
            "flag",
            None,
            &format!("Deleted flag {key} and its arms."),
        )
        .await;
        Ok(Json(Empty {}))
    }
}

pub(super) fn percent(raw: u32) -> Result<u32, V2Error> {
    if raw > 100 {
        return Err(V2Error::invalid_argument(
            "A rollout is a percentage between 0 and 100.",
        ));
    }
    Ok(raw)
}

pub(super) fn admin_above(claims: &ConsoleAuthed, pct: u32) -> Result<(), V2Error> {
    if pct > ADMIN_ABOVE {
        return claims.require_admin();
    }
    Ok(())
}

fn description(raw: &str) -> Result<String, V2Error> {
    let text = raw.trim();
    if text.len() > 200 {
        return Err(V2Error::invalid_argument(
            "A flag's description is up to 200 characters.",
        ));
    }
    Ok(text.to_owned())
}

/// A body that says nothing about the description keeps what is written, so a
/// PUT that switches a flag off does not quietly delete why it exists.
fn described(raw: &Option<String>, keep: &str) -> Result<String, V2Error> {
    let kept = raw
        .as_deref()
        .map(description)
        .transpose()?
        .unwrap_or_else(|| keep.to_owned());
    Ok(kept)
}

fn salt(raw: Option<&str>, keep: Option<&str>) -> Result<String, V2Error> {
    match raw.map(str::trim).filter(|salt| !salt.is_empty()) {
        Some(salt) if salt.len() > 64 => Err(V2Error::invalid_argument(
            "A flag's salt is up to 64 characters.",
        )),
        Some(salt) => Ok(salt.to_owned()),
        None => Ok(keep.map_or_else(|| Uuid::new_v4().to_string(), str::to_owned)),
    }
}

/// A window nobody can be inside is a mistake rather than a schedule: the flag
/// would read as switched off forever with nothing on the row saying why.
fn scheduled(row: FeatureFlag) -> Result<FeatureFlag, V2Error> {
    if let (Some(from), Some(to)) = (&row.active_from, &row.active_to)
        && to <= from
    {
        return Err(V2Error::invalid_argument(
            "A flag's window has to open before it closes.",
        ));
    }
    Ok(row)
}
