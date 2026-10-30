use std::collections::BTreeMap;
use std::sync::Arc;

use auto_di::singleton;
use axum::Json;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::quest::{ClaimQuestRequest, QuestClaimResponse, QuestListResponse, QuestRow};
use crate::error::V2Error;
use crate::repositories::db::Queries;
use crate::repositories::models::QuestDefinition;
use crate::services::deps::q;

pub(crate) mod catalog;
pub mod daily;
pub mod events;
pub(crate) mod payout;
pub(crate) mod reward;

pub(crate) use catalog::{
    definition_of, entitled, open_seasons, owned_definitions, progress_by_quest, runs_now,
    stats_by_player,
};
pub(crate) use reward::parse_reward;

pub struct QuestService;

#[singleton]
impl QuestService {
    fn new() -> Self {
        Self
    }
}

impl QuestService {
    pub async fn list_quests(
        &self,
        state: &ApiState,
        auth: &Authed,
    ) -> Result<Json<QuestListResponse>, V2Error> {
        let queries = q(state).await?;
        let now = Utc::now();
        let progress = progress_by_quest(&queries, auth.uid()).await?;
        let stats = stats_by_player(&queries, auth.uid()).await?;
        let seasons = open_seasons(&queries, now).await?;
        let products =
            crate::services::iap::entitlements::active_products(&queries, auth.uid(), now).await?;
        let quests = entitled(
            &owned_definitions(&queries, state).await?,
            &seasons,
            now,
            &products,
        )
        .into_iter()
        .map(|definition| {
            QuestRow::new(
                definition,
                progress.get(&definition.id),
                counted(definition, &stats),
            )
        })
        .collect();
        Ok(Json(QuestListResponse { quests }))
    }

    pub async fn claim_quest(
        &self,
        state: &ApiState,
        auth: &Authed,
        req: ClaimQuestRequest,
    ) -> Result<Json<QuestClaimResponse>, V2Error> {
        let quest_key = req.quest_key.trim();
        if quest_key.is_empty() {
            return Err(V2Error::invalid_argument("A quest needs a key."));
        }
        let queries = q(state).await?;
        let now = Utc::now();
        let definition = definition_of(&queries, state, quest_key).await?;
        if !runs_now(&definition, &open_seasons(&queries, now).await?, now) {
            return Err(V2Error::failed_precondition(
                "This quest is outside its running window.",
            ));
        }
        if let Some(product) = definition.requires_product.as_deref()
            && !subscribed_to(&queries, auth.uid(), now, product).await?
        {
            return Err(V2Error::failed_precondition(
                "This quest is part of a subscription track that is not active.",
            ));
        }
        let reward = parse_reward(&definition.reward)?;
        if reward.is_empty() {
            return Err(V2Error::failed_precondition(
                "This quest has no reward to claim.",
            ));
        }
        let counted = queries
            .get_player_stat(auth.uid(), &definition.goal_kind)
            .await?
            .map_or(0, |row| row.value);
        // Flipping the row before anything is paid is what makes a second claim
        // — retried, concurrent or malicious — refuse instead of paying twice.
        if counted >= definition.goal_amount {
            queries
                .project_player_quest(auth.uid(), quest_key, counted, "completed")
                .await?;
        }
        let taken = match queries
            .claim_player_quest(auth.uid(), quest_key, definition.goal_amount)
            .await?
        {
            Some(row) => row,
            None if counted >= definition.goal_amount => {
                return Err(V2Error::failed_precondition(
                    "This quest's reward is already claimed.",
                ));
            }
            None => {
                return Err(V2Error::failed_precondition(
                    "This quest is not finished yet.",
                ));
            }
        };
        let seed = format!("quest:{}:{quest_key}", auth.uid());
        let granted = match payout::pay(&queries, auth.uid(), &reward, &seed).await {
            Ok(granted) => granted,
            Err(err) => {
                // Nothing was paid, so the quest goes back on the board.
                let _ = queries.release_player_quest(auth.uid(), quest_key).await;
                return Err(err);
            }
        };
        Ok(Json(QuestClaimResponse {
            success: true,
            message: format!("Claimed {quest_key}."),
            quest: QuestRow::new(&definition, Some(&taken), counted),
            reward: granted,
        }))
    }
}

/// What a quest has counted so far: the counter it watches, not the row it left
/// behind, so a quest that finished while its season was closed reads true the
/// moment the season opens.
fn counted(definition: &QuestDefinition, stats: &BTreeMap<String, i64>) -> i64 {
    stats.get(&definition.goal_kind).copied().unwrap_or(0)
}

async fn subscribed_to(
    queries: &Arc<Queries>,
    user_id: Uuid,
    now: DateTime<Utc>,
    product: &str,
) -> Result<bool, V2Error> {
    Ok(
        crate::services::iap::entitlements::active_products(queries, user_id, now)
            .await?
            .contains(product),
    )
}
