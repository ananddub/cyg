use axum::Json;
use chrono::{DateTime, Utc};

use crate::ApiState;
use crate::app::middleware::ConsoleAuthed;
use crate::dto::Empty;
use crate::dto::badge::{BadgeDefinitionRequest, BadgeDefinitionRow};
use crate::error::V2Error;
use crate::repositories::models::BadgeDefinition;
use crate::services::deps::q;
use crate::services::quest::catalog::{counter_name, definition_key};

impl super::ConsoleService {
    pub async fn list_badges(
        &self,
        state: &ApiState,
    ) -> Result<Json<Vec<BadgeDefinitionRow>>, V2Error> {
        let rows = q(state).await?.list_badge_definitions().await?;
        Ok(Json(
            rows.into_iter().map(BadgeDefinitionRow::from).collect(),
        ))
    }

    pub async fn create_badge(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        body: BadgeDefinitionRequest,
    ) -> Result<Json<BadgeDefinitionRow>, V2Error> {
        claims.require_write()?;
        let key = definition_key(&body.badge_key, "badge")?;
        let queries = q(state).await?;
        let draft = draft(state, key.clone(), body, Utc::now()).await?;
        if !queries.create_badge_definition(&draft).await? {
            return Err(V2Error::already_exists(
                "A badge with this key already exists.",
            ));
        }
        super::audit(
            state,
            claims,
            "badge.create",
            "badge",
            None,
            &format!("Created badge {key}."),
        )
        .await;
        Ok(Json(BadgeDefinitionRow::from(draft)))
    }

    pub async fn update_badge(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        name: &str,
        body: BadgeDefinitionRequest,
    ) -> Result<Json<BadgeDefinitionRow>, V2Error> {
        claims.require_write()?;
        let key = definition_key(name, "badge")?;
        let queries = q(state).await?;
        let existing = queries.get_badge_definition(&key).await?;
        let draft = draft(
            state,
            key.clone(),
            body,
            existing.map_or_else(Utc::now, |row| row.create_time),
        )
        .await?;
        queries.update_badge_definition(&draft).await?;
        super::audit(
            state,
            claims,
            "badge.update",
            "badge",
            None,
            &format!("Wrote badge {key}."),
        )
        .await;
        Ok(Json(BadgeDefinitionRow::from(draft)))
    }

    pub async fn delete_badge(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        name: &str,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_write()?;
        let key = definition_key(name, "badge")?;
        let queries = q(state).await?;
        if queries.delete_badge_definition(&key).await?.rows_affected() == 0 {
            return Err(V2Error::not_found("Badge not found."));
        }
        super::audit(
            state,
            claims,
            "badge.delete",
            "badge",
            None,
            &format!("Deleted badge {key}."),
        )
        .await;
        Ok(Json(Empty {}))
    }
}

/// Validates what the operator typed and freezes it into a row. Writing a badge
/// that names no counter is allowed — it simply has nothing to read yet — but
/// the name has to be one an event could raise.
async fn draft(
    state: &ApiState,
    id: String,
    body: BadgeDefinitionRequest,
    create_time: DateTime<Utc>,
) -> Result<BadgeDefinition, V2Error> {
    let title = body.title.trim();
    if title.is_empty() || title.len() > 120 {
        return Err(V2Error::invalid_argument(
            "A badge needs a title of up to 120 characters.",
        ));
    }
    let detail = body.detail.trim();
    if detail.len() > 500 {
        return Err(V2Error::invalid_argument(
            "A badge's detail is up to 500 characters.",
        ));
    }
    let stat = counter_name(&body.stat)?.to_string();
    if body.threshold < 1 {
        return Err(V2Error::invalid_argument(
            "A badge's threshold must be at least 1.",
        ));
    }
    Ok(BadgeDefinition {
        id,
        game_id: crate::services::games::tenant::game_id(state).await?,
        title: title.to_string(),
        detail: detail.to_string(),
        stat,
        threshold: body.threshold,
        is_active: body.is_active.unwrap_or(true),
        create_time,
        update_time: Utc::now(),
    })
}
