use axum::Json;
use chrono::Utc;

use crate::ApiState;
use crate::app::middleware::ConsoleAuthed;
use crate::dto::Empty;
use crate::dto::flag::{VariantRequest, VariantRow};
use crate::error::V2Error;
use crate::repositories::models::FlagVariant;
use crate::services::deps::q;
use crate::services::quest::catalog::definition_key;

/// How many arms one flag may split its rollout across, and the largest weight
/// an arm may carry. Both are far above what a test needs and far below what
/// would make a flag list unreadable.
const MAX_ARMS: usize = 16;
const MAX_WEIGHT: u32 = 1_000;

impl super::ConsoleService {
    pub async fn list_variants(
        &self,
        state: &ApiState,
        raw_key: &str,
    ) -> Result<Json<Vec<VariantRow>>, V2Error> {
        let key = definition_key(raw_key, "flag")?;
        let arms = q(state).await?.arms_of(&key).await?;
        Ok(Json(
            arms.into_iter()
                .enumerate()
                .map(|(index, arm)| VariantRow::new(arm, index == 0))
                .collect(),
        ))
    }

    /// Adds an arm. A flag that already has one by that name would be a flag
    /// nobody can read, so this reports the collision instead of writing over it.
    pub async fn create_variant(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        raw_key: &str,
        body: VariantRequest,
    ) -> Result<Json<VariantRow>, V2Error> {
        claims.require_write()?;
        let key = definition_key(raw_key, "flag")?;
        let queries = q(state).await?;
        defined(state, &key).await?;
        let name = definition_key(body.name.as_deref().unwrap_or_default(), "variant")?;
        let arm = arm(&key, &name, body.weight)?;
        let arms = queries.arms_of(&key).await?;
        if arms.len() + 1 > MAX_ARMS {
            return Err(V2Error::invalid_argument(format!(
                "A flag has at most {MAX_ARMS} arms."
            )));
        }
        if !queries.create_variant(&arm).await? {
            return Err(V2Error::already_exists(
                "This flag already has an arm with that name.",
            ));
        }
        audit_arm(
            state,
            claims,
            "flag.variant.create",
            &format!(
                "Added {name} to flag {key} as its {} arm.",
                if arms.is_empty() {
                    "control"
                } else {
                    "treatment"
                }
            ),
        )
        .await;
        Ok(Json(VariantRow::new(arm, arms.is_empty())))
    }

    /// Adds an arm or re-weights one that exists. Retuning weights moves
    /// accounts between arms, which is why the split is only ever decided by the
    /// hash at read time and never stored against an account.
    pub async fn put_variant(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        raw_key: &str,
        raw_name: &str,
        body: VariantRequest,
    ) -> Result<Json<VariantRow>, V2Error> {
        claims.require_write()?;
        let key = definition_key(raw_key, "flag")?;
        let queries = q(state).await?;
        defined(state, &key).await?;
        let name = definition_key(raw_name, "variant")?;
        let arm = arm(&key, &name, body.weight)?;
        let before = queries.arms_of(&key).await?;
        queries.update_variant(&arm).await?;
        audit_arm(
            state,
            claims,
            "flag.variant.update",
            &format!("Set flag {key} arm {name} to weight {}.", arm.weight),
        )
        .await;
        Ok(Json(VariantRow::new(
            arm,
            before.first().is_some_and(|first| first.name == name),
        )))
    }

    pub async fn delete_variant(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        raw_key: &str,
        raw_name: &str,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_write()?;
        let key = definition_key(raw_key, "flag")?;
        let name = definition_key(raw_name, "variant")?;
        if q(state)
            .await?
            .delete_variant(&key, &name)
            .await?
            .rows_affected()
            == 0
        {
            return Err(V2Error::not_found("Arm not found."));
        }
        audit_arm(
            state,
            claims,
            "flag.variant.delete",
            &format!("Removed {name} from flag {key}."),
        )
        .await;
        Ok(Json(Empty {}))
    }
}

/// An arm or a pin needs a flag to belong to: without this check a route could
/// keep writing parts for a key nothing reads back once the experiment is gone.
pub(super) async fn defined(state: &ApiState, key: &str) -> Result<(), V2Error> {
    if q(state).await?.get_flag(key).await?.is_none() {
        return Err(V2Error::not_found("Flag not found."));
    }
    Ok(())
}

fn arm(flag_key: &str, name: &str, weight: u32) -> Result<FlagVariant, V2Error> {
    if weight > MAX_WEIGHT {
        return Err(V2Error::invalid_argument(format!(
            "An arm's weight is between 0 and {MAX_WEIGHT}."
        )));
    }
    Ok(FlagVariant {
        id: format!("{flag_key}:{name}"),
        flag_key: flag_key.to_owned(),
        name: name.to_owned(),
        weight,
        update_time: Utc::now(),
    })
}

async fn audit_arm(state: &ApiState, claims: &ConsoleAuthed, action: &str, message: &str) {
    super::audit(state, claims, action, "flag", None, message).await;
}
