use axum::Json;
use chrono::Utc;

use super::flag_arms::defined;
use crate::ApiState;
use crate::app::middleware::ConsoleAuthed;
use crate::dto::Empty;
use crate::dto::flag::{OverrideRequest, OverrideRow};
use crate::error::V2Error;
use crate::repositories::models::FlagOverride;
use crate::services::deps::q;
use crate::services::quest::catalog::definition_key;
use crate::utils::ids::parse_uuid;

impl super::ConsoleService {
    pub async fn list_overrides(
        &self,
        state: &ApiState,
        raw_key: &str,
    ) -> Result<Json<Vec<OverrideRow>>, V2Error> {
        let key = definition_key(raw_key, "flag")?;
        let rows = q(state).await?.list_overrides(&key).await?;
        Ok(Json(rows.into_iter().map(OverrideRow::from).collect()))
    }

    /// Pins one account to one arm. A/B is a statement about a population, and
    /// this is the exception written for one player: the arm they are told about
    /// stops depending on the hash from here.
    pub async fn set_override(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        raw_key: &str,
        body: OverrideRequest,
    ) -> Result<Json<OverrideRow>, V2Error> {
        claims.require_write()?;
        let key = definition_key(raw_key, "flag")?;
        defined(state, &key).await?;
        let user_id = parse_uuid(&body.user_id, "account")?;
        let variant = definition_key(&body.variant, "variant")?;
        let queries = q(state).await?;
        let arms = queries.arms_of(&key).await?;
        if !arms.is_empty() && !arms.iter().any(|arm| arm.name == variant) {
            return Err(V2Error::invalid_argument(
                "This flag has no arm with that name.",
            ));
        }
        let row = FlagOverride {
            id: format!("{key}:{user_id}"),
            flag_key: key.clone(),
            user_id,
            variant: variant.clone(),
            update_time: Utc::now(),
        };
        queries.upsert_override(&row).await?;
        super::audit(
            state,
            claims,
            "flag.override",
            "flag",
            Some(user_id),
            &format!("Pinned an account to {variant} on flag {key}."),
        )
        .await;
        Ok(Json(OverrideRow::from(row)))
    }

    pub async fn delete_override(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        raw_key: &str,
        raw_user: &str,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_write()?;
        let key = definition_key(raw_key, "flag")?;
        let user_id = parse_uuid(raw_user, "account")?;
        if q(state)
            .await?
            .delete_override(&key, user_id)
            .await?
            .rows_affected()
            == 0
        {
            return Err(V2Error::not_found("Override not found."));
        }
        super::audit(
            state,
            claims,
            "flag.override.delete",
            "flag",
            Some(user_id),
            &format!("Released an account from flag {key}."),
        )
        .await;
        Ok(Json(Empty {}))
    }
}
