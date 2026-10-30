use std::collections::BTreeMap;

use axum::Json;

use super::MatchResultService;
use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::badge::{BadgeListResponse, BadgeRow};
use crate::error::V2Error;
use crate::repositories::models::BadgeDefinition;
use crate::services::deps::q;
use crate::services::quest::catalog::stats_by_player;

impl MatchResultService {
    /// Every badge this game defines, with the player's counter beside each
    /// threshold. Earning is decided here, at read time, from `player_stat`, so a
    /// re-tuned threshold re-judges every player without rewriting a row.
    pub async fn list_badges(
        &self,
        state: &ApiState,
        auth: &Authed,
    ) -> Result<Json<BadgeListResponse>, V2Error> {
        let queries = q(state).await?;
        let game_id = crate::services::games::tenant::game_id(state).await?;
        let stats = stats_by_player(&queries, auth.uid()).await?;
        let rows = queries.list_badge_definitions().await?;
        Ok(Json(BadgeListResponse {
            badges: rows
                .iter()
                .filter(|badge| badge.is_active)
                .filter(|badge| {
                    badge.game_id.is_none() || game_id.is_none() || badge.game_id == game_id
                })
                .map(|badge| earned(badge, &stats))
                .collect(),
        }))
    }
}

/// One badge as the player stands: the counter it reads, the value it holds, and
/// whether that reaches the threshold. A player with no such counter holds zero.
pub(crate) fn earned(badge: &BadgeDefinition, stats: &BTreeMap<String, i64>) -> BadgeRow {
    let value = stats.get(&badge.stat).copied().unwrap_or(0);
    BadgeRow {
        badge_key: badge.id.clone(),
        title: badge.title.clone(),
        detail: badge.detail.clone(),
        stat: badge.stat.clone(),
        threshold: badge.threshold,
        value,
        earned: value >= badge.threshold,
    }
}
