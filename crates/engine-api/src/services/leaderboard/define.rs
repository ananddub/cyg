//! Creating, listing and deleting boards from the game's own code.
//!
//! A board can be declared in the console, but a game that ships its own
//! leaderboards should not need an operator to click them into existence first —
//! Nakama's `leaderboardCreate` and PlayFab's `CreateLeaderboard` exist because a
//! season board is part of the game's content, not of its deployment.

use serde_json::Value;

use crate::app::middleware::Authed;
use crate::error::V2Error;
use crate::repositories::leaderboard::DefineLeaderboardParams;
use crate::repositories::models::Leaderboard;

use super::{LeaderboardService, load_board};

/// What the game may say about a board it creates.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct DefineRequest {
    /// Board id, e.g. `weekly_kills`. The path segment is authoritative when both
    /// are present, so a body copied from another board cannot rename this one.
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    /// `1` = higher score wins, `-1` = lower wins.
    #[serde(default)]
    pub sort_order: i16,
    /// `0` = overwrite the score, otherwise "best of".
    #[serde(default)]
    pub operator: i16,
    /// How often scores reset. Empty means never.
    #[serde(default)]
    pub reset_schedule: String,
    #[serde(default)]
    pub metadata: Value,
    /// When true the server computes the score; a client may not submit one.
    #[serde(default)]
    pub authoritative: bool,
    #[serde(default)]
    pub category: i16,
    /// How many scores to keep per player. `0` keeps one.
    #[serde(default)]
    pub max_num_score: i32,
}

impl DefineRequest {
    fn normalise(&mut self, path_id: &str) {
        if !path_id.is_empty() {
            self.id = path_id.to_string();
        }
        self.id = self.id.trim().to_string();
        if self.title.trim().is_empty() {
            self.title = self.id.clone();
        }
        // Anything but an explicit "lower wins" means higher wins, which is what
        // an unset field and a zero both mean.
        self.sort_order = if self.sort_order == -1 { -1 } else { 1 };
        if self.operator != 0 {
            self.operator = 1;
        }
        if self.max_num_score <= 0 {
            self.max_num_score = 1;
        }
        // Reset schedules are a closed set; an unknown one would silently never
        // fire, which looks identical to "never resets".
        const SCHEDULES: [&str; 4] = ["", "daily", "weekly", "monthly"];
        if !SCHEDULES.contains(&self.reset_schedule.trim().to_ascii_lowercase().as_str()) {
            self.reset_schedule.clear();
        } else {
            self.reset_schedule = self.reset_schedule.trim().to_ascii_lowercase();
        }
    }
}

impl LeaderboardService {
    /// Create or replace a board.
    ///
    /// Idempotent by design: a game that declares its boards at boot calls this
    /// every time it starts, and only the fields it states are overwritten.
    pub async fn define_board(
        &self,
        state: &crate::ApiState,
        _auth: &Authed,
        path_id: &str,
        mut body: DefineRequest,
    ) -> Result<serde_json::Value, V2Error> {
        body.normalise(path_id);
        if body.id.is_empty() {
            return Err(V2Error::invalid_argument(
                "a board needs an id: send it as the path segment or in the body",
            ));
        }
        // The id becomes a record id, so it must not carry path syntax.
        if body.id.contains(['/', '\\']) || body.id.contains("..") {
            return Err(V2Error::invalid_argument(
                "board id may not contain '/', '\\' or '..'",
            ));
        }

        let queries = crate::services::deps::q(state).await?;
        let params = DefineLeaderboardParams {
            id: body.id.clone(),
            title: body.title,
            description: body.description,
            operator: body.operator,
            sort_order: body.sort_order,
            reset_schedule: body.reset_schedule,
            metadata: body.metadata,
            authoritative: body.authoritative,
            category: body.category,
            max_num_score: body.max_num_score,
        };
        let row = queries
            .define_leaderboard(&params)
            .await
            .map_err(V2Error::from)?;
        Ok(board_json(&row))
    }

    /// Every board the game has defined.
    pub async fn list_boards(
        &self,
        state: &crate::ApiState,
        _auth: &Authed,
    ) -> Result<serde_json::Value, V2Error> {
        let queries = crate::services::deps::q(state).await?;
        let rows = queries.list_leaderboards(i64::MAX, 0).await?;
        let boards: Vec<Value> = rows.iter().map(board_json).collect();
        Ok(serde_json::json!({ "leaderboards": boards, "total": boards.len() }))
    }

    /// Remove a board and every score on it.
    ///
    /// Refused when the id does not exist, so a typo reports itself rather than
    /// looking like a successful delete.
    pub async fn delete_board(
        &self,
        state: &crate::ApiState,
        _auth: &Authed,
        id: &str,
    ) -> Result<Value, V2Error> {
        let queries = crate::services::deps::q(state).await?;
        load_board(&queries, id).await?;
        queries.delete_leaderboard(id).await?;
        Ok(serde_json::json!({ "deleted": id }))
    }
}

fn board_json(row: &Leaderboard) -> Value {
    serde_json::json!({
        "id": row.id,
        "title": row.title,
        "description": row.description,
        "sort_order": row.sort_order,
        "operator": row.operator,
        "reset_schedule": row.reset_schedule,
        "metadata": row.metadata,
        "authoritative": row.authoritative,
        "category": row.category,
        "max_num_score": row.max_num_score,
    })
}
