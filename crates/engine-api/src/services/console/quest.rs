use std::sync::Arc;

use axum::Json;
use chrono::{DateTime, NaiveDate, NaiveTime, Utc};

use crate::ApiState;
use crate::app::middleware::ConsoleAuthed;
use crate::dto::Empty;
use crate::dto::console::quest::{QuestDefinitionRequest, QuestDefinitionRow};
use crate::error::V2Error;
use crate::repositories::db::Queries;
use crate::repositories::models::QuestDefinition;
use crate::services::deps::q;
use crate::services::quest::catalog::{counter_name, definition_key};
use crate::services::quest::reward::parse_reward;

impl super::ConsoleService {
    pub async fn list_quests(
        &self,
        state: &ApiState,
    ) -> Result<Json<Vec<QuestDefinitionRow>>, V2Error> {
        let rows = q(state).await?.list_quest_definitions().await?;
        Ok(Json(
            rows.into_iter().map(QuestDefinitionRow::from).collect(),
        ))
    }

    pub async fn create_quest(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        body: QuestDefinitionRequest,
    ) -> Result<Json<QuestDefinitionRow>, V2Error> {
        claims.require_write()?;
        let key = definition_key(&body.quest_key, "quest")?;
        let queries = q(state).await?;
        let draft = draft(&queries, state, key.clone(), body, Utc::now()).await?;
        if !queries.create_quest_definition(&draft).await? {
            return Err(V2Error::already_exists(
                "A quest with this key already exists.",
            ));
        }
        super::audit(
            state,
            claims,
            "quest.create",
            "quest",
            None,
            &format!("Created quest {key}."),
        )
        .await;
        Ok(Json(QuestDefinitionRow::from(draft)))
    }

    pub async fn update_quest(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        name: &str,
        body: QuestDefinitionRequest,
    ) -> Result<Json<QuestDefinitionRow>, V2Error> {
        claims.require_write()?;
        let key = definition_key(name, "quest")?;
        let queries = q(state).await?;
        let existing = queries.get_quest_definition(&key).await?;
        let draft = draft(
            &queries,
            state,
            key.clone(),
            body,
            existing.map_or_else(Utc::now, |row| row.create_time),
        )
        .await?;
        queries.update_quest_definition(&draft).await?;
        super::audit(
            state,
            claims,
            "quest.update",
            "quest",
            None,
            &format!("Wrote quest {key}."),
        )
        .await;
        Ok(Json(QuestDefinitionRow::from(draft)))
    }

    pub async fn delete_quest(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        name: &str,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_write()?;
        let key = definition_key(name, "quest")?;
        let queries = q(state).await?;
        if queries.delete_quest_definition(&key).await?.rows_affected() == 0 {
            return Err(V2Error::not_found("Quest not found."));
        }
        super::audit(
            state,
            claims,
            "quest.delete",
            "quest",
            None,
            &format!("Deleted quest {key}."),
        )
        .await;
        Ok(Json(Empty {}))
    }
}

/// Validates what the operator typed and freezes it into a row. A quest that
/// cannot be parsed here can never fail a player's claim later.
async fn draft(
    queries: &Arc<Queries>,
    state: &ApiState,
    id: String,
    body: QuestDefinitionRequest,
    create_time: DateTime<Utc>,
) -> Result<QuestDefinition, V2Error> {
    let title = body.title.trim();
    if title.is_empty() || title.len() > 120 {
        return Err(V2Error::invalid_argument(
            "A quest needs a title of up to 120 characters.",
        ));
    }
    let goal_kind = counter_name(&body.goal_kind)?.to_string();
    if body.goal_amount < 1 {
        return Err(V2Error::invalid_argument(
            "A quest's goal amount must be at least 1.",
        ));
    }
    if parse_reward(&body.reward)?.is_empty() {
        return Err(V2Error::invalid_argument(
            "A quest needs a reward of currency or items.",
        ));
    }
    let season = match body.season.as_deref() {
        None => None,
        Some(raw) => {
            let key = definition_key(raw, "season")?;
            if queries.get_season(&key).await?.is_none() {
                return Err(V2Error::invalid_argument(format!(
                    "Season '{key}' does not exist."
                )));
            }
            Some(key)
        }
    };
    let requires_product = match body.requires_product.as_deref() {
        None => None,
        Some(raw) => {
            let product = raw.trim();
            if product.is_empty() || product.len() > 128 {
                return Err(V2Error::invalid_argument(
                    "A quest's required product is 1-128 characters.",
                ));
            }
            Some(product.to_string())
        }
    };
    let active_from = body.active_from.unwrap_or_else(super::never);
    let active_to = body.active_to.unwrap_or_else(open_ended);
    if active_from >= active_to {
        return Err(V2Error::invalid_argument(
            "A quest's window has to end after it starts.",
        ));
    }
    Ok(QuestDefinition {
        id,
        game_id: crate::services::games::tenant::game_id(state).await?,
        title: title.to_string(),
        goal_kind,
        goal_amount: body.goal_amount,
        reward: body.reward,
        season,
        requires_product,
        active_from,
        active_to,
        create_time,
        update_time: Utc::now(),
    })
}

fn open_ended() -> DateTime<Utc> {
    NaiveDate::from_ymd_opt(9999, 12, 31)
        .expect("the last day exists")
        .and_time(NaiveTime::MIN)
        .and_utc()
}
