use axum::Json;
use chrono::{Datelike, Utc};

use super::QuestService;
use super::{payout, reward::parse_reward};
use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::quest::DailyClaimResponse;
use crate::error::V2Error;
use crate::repositories::db::Queries;
use crate::services::deps::q;
use std::sync::Arc;

/// The marker holding the last UTC day this player took the daily reward.
const DAILY_STAT: &str = "daily_claim";
/// Optional operator-authored reward; the default is a small currency grant.
const DAILY_SETTING: &str = "daily_reward";

impl QuestService {
    pub async fn claim_daily(
        &self,
        state: &ApiState,
        auth: &Authed,
    ) -> Result<Json<DailyClaimResponse>, V2Error> {
        let queries = q(state).await?;
        let today = Utc::now().date_naive().num_days_from_ce() as i64;
        let reward = parse_reward(&daily_reward(&queries).await?)?;
        if reward.is_empty() {
            return Err(V2Error::failed_precondition(
                "No daily reward is configured for this game.",
            ));
        }
        let held = queries.get_player_stat(auth.uid(), DAILY_STAT).await?;
        if held.as_ref().is_some_and(|row| row.value >= today) {
            return Err(V2Error::already_exists(
                "The daily reward has already been claimed today.",
            ));
        }
        if !queries
            .mark_player_day(auth.uid(), DAILY_STAT, today)
            .await?
        {
            return Err(V2Error::already_exists(
                "The daily reward has already been claimed today.",
            ));
        }
        // The reward's own content keeps the seed, so replaying a day against an
        // unchanged config touches one ledger row, while an edited config pays new.
        let seed = format!("daily:{}:{today}:{}", auth.uid(), reward.summary());
        match payout::pay(&queries, auth.uid(), &reward, &seed).await {
            Ok(granted) => Ok(Json(DailyClaimResponse {
                success: true,
                message: "Daily reward claimed.".to_string(),
                day: today,
                reward: granted,
            })),
            Err(err) => {
                // Nothing was paid, so the day opens up again for a retry.
                let _ = queries
                    .rewind_player_stat(auth.uid(), DAILY_STAT, held.map(|row| row.value))
                    .await;
                Err(err)
            }
        }
    }
}

async fn daily_reward(queries: &Arc<Queries>) -> Result<serde_json::Value, V2Error> {
    Ok(queries
        .setting_value(DAILY_SETTING)
        .await?
        .unwrap_or_else(|| serde_json::json!({ "currency": { "gold": 100 } })))
}
