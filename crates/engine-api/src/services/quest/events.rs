use std::sync::Arc;

use axum::Json;

use super::catalog::{counter_name, open_seasons, owned_definitions, visible};
use crate::ApiState;
use crate::db::DbError;
use crate::dto::quest::{CompletedQuest, IngestEventsRequest, IngestEventsResponse};
use crate::error::V2Error;
use crate::repositories::db::Queries;
use crate::repositories::models::QuestDefinition;
use crate::services::deps::q;
use uuid::Uuid;

/// The largest counter a single event may add, so one bad payload cannot mint
/// progress out of nowhere.
const MAX_INCREMENT: i64 = 1_000_000;
const MAX_BATCH: usize = 100;

/// Credits one counter increment and re-projects every visible quest that
/// counts it: the counter — not the event — is what a quest reads, so a lost
/// event can be made up by sending the same counter again. A quest that crosses
/// its goal on this increment fires the `quest.completed` webhook and lands in
/// the returned list. Shared by the server-key ingest door and the mirrored
/// gameplay-event pump.
pub(crate) async fn credit_counter(
    state: &ApiState,
    queries: &Arc<Queries>,
    user_id: Uuid,
    name: &str,
    delta: i64,
    quests: &[&QuestDefinition],
) -> Result<Vec<CompletedQuest>, DbError> {
    let total = queries.increase_player_stat(user_id, name, delta).await?;
    // A level is the `level` counter, so a level-up is that counter crossing a
    // new number. Fired here rather than from a level service because there is
    // no level service: the counter is the level.
    if name == "level" && total > total - delta {
        crate::services::rules::dispatch::fire(state, "level_up", user_id).await;
    }
    let mut completed = Vec::new();
    for quest in quests.iter().filter(|quest| quest.goal_kind == name) {
        let crosses = total >= quest.goal_amount && total - delta < quest.goal_amount;
        let mark = if total >= quest.goal_amount {
            "completed"
        } else {
            "in_progress"
        };
        let projected = queries
            .project_player_quest(user_id, &quest.id, total, mark)
            .await?;
        if crosses && projected.is_some() {
            crate::services::webhook::emit(
                state,
                "quest.completed",
                &format!("{user_id}:{}", quest.id),
                serde_json::json!({ "user_id": user_id, "quest_key": quest.id }),
            )
            .await;
            completed.push(CompletedQuest {
                user_id,
                quest_key: quest.id.clone(),
            });
        }
    }
    Ok(completed)
}

impl super::QuestService {
    /// Server-authored events. Each one raises a counter and re-projects every
    /// quest that counts it: the counter — not the event — is what a quest reads,
    /// so a lost event can be made up by sending the same counter again.
    pub async fn ingest_events(
        &self,
        state: &ApiState,
        req: IngestEventsRequest,
    ) -> Result<Json<IngestEventsResponse>, V2Error> {
        if req.events.is_empty() {
            return Err(V2Error::invalid_argument("At least one event is required."));
        }
        if req.events.len() > MAX_BATCH {
            return Err(V2Error::invalid_argument(format!(
                "An event batch holds at most {MAX_BATCH} events."
            )));
        }
        let queries = q(state).await?;
        let mut credited = Vec::with_capacity(req.events.len());
        for event in &req.events {
            let name = counter_name(&event.name)?;
            let delta = event.value.unwrap_or(1);
            if !(1..=MAX_INCREMENT).contains(&delta) {
                return Err(V2Error::invalid_argument(format!(
                    "The {name} increment must be between 1 and {MAX_INCREMENT}."
                )));
            }
            if let Err(DbError::NotFound) = queries.get_user_by_id(event.user_id).await {
                return Err(V2Error::not_found("Account not found."));
            }
            credited.push((name, delta, event.user_id));
        }
        // The whole batch is checked before the first counter moves, so one
        // unusable event cannot leave a half-credited batch behind.
        let now = chrono::Utc::now();
        let seasons = open_seasons(&queries, now).await?;
        let definitions = owned_definitions(&queries, state).await?;
        let quests = visible(&definitions, &seasons, now);
        let mut completed = Vec::new();
        for (name, delta, user_id) in credited {
            completed
                .extend(credit_counter(state, &queries, user_id, &name, delta, &quests).await?);
        }
        Ok(Json(IngestEventsResponse {
            success: true,
            accepted: req.events.len(),
            completed,
        }))
    }
}
