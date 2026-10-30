use super::*;

/// The record id of a player's progress on one quest. Derived from the pair,
/// so a player can only ever hold one progress row per quest.
pub fn player_quest_id(user_id: Uuid, quest_key: &str) -> String {
    format!("{user_id}:{quest_key}")
}

/// The record id of one counter for one player.
pub fn player_stat_id(user_id: Uuid, stat: &str) -> String {
    format!("{user_id}:{stat}")
}

impl Queries {
    pub async fn list_quest_definitions(&self) -> Result<Vec<QuestDefinition>, DbError> {
        self.all("SELECT * FROM quest_definition ORDER BY id ASC", params!())
            .await
    }

    pub async fn get_quest_definition(
        &self,
        quest_key: &str,
    ) -> Result<Option<QuestDefinition>, DbError> {
        self.opt(
            "SELECT * FROM type::record('quest_definition', $id) LIMIT 1",
            params!(id = quest_key),
        )
        .await
    }

    /// Creates a definition, reporting `false` when the key is already taken.
    pub async fn create_quest_definition(&self, row: &QuestDefinition) -> Result<bool, DbError> {
        match self
            .create("quest_definition", Key::String(row.id.clone()), row)
            .await
        {
            Ok(_) => Ok(true),
            Err(err) if err.is_unique() => Ok(false),
            Err(err) => Err(err),
        }
    }

    pub async fn update_quest_definition(&self, row: &QuestDefinition) -> Result<(), DbError> {
        self.exec(
            "UPSERT type::record('quest_definition', $id) CONTENT $data",
            params!(id = row.id.clone(), data = row),
        )
        .await
    }

    pub async fn delete_quest_definition(&self, quest_key: &str) -> Result<WriteResult, DbError> {
        affected(
            self,
            "DELETE type::record('quest_definition', $id) RETURN BEFORE",
            params!(id = quest_key),
        )
        .await
    }

    pub async fn list_seasons(&self) -> Result<Vec<Season>, DbError> {
        self.all("SELECT * FROM season ORDER BY starts_at ASC", params!())
            .await
    }

    pub async fn get_season(&self, season_key: &str) -> Result<Option<Season>, DbError> {
        self.opt(
            "SELECT * FROM type::record('season', $id) LIMIT 1",
            params!(id = season_key),
        )
        .await
    }

    pub async fn upsert_season(&self, row: &Season) -> Result<(), DbError> {
        self.exec(
            "UPSERT type::record('season', $id) CONTENT $data",
            params!(id = row.id.clone(), data = row),
        )
        .await
    }

    pub async fn delete_season(&self, season_key: &str) -> Result<WriteResult, DbError> {
        affected(
            self,
            "DELETE type::record('season', $id) RETURN BEFORE",
            params!(id = season_key),
        )
        .await
    }

    pub async fn list_player_quests(&self, user_id: Uuid) -> Result<Vec<PlayerQuest>, DbError> {
        self.all(
            "SELECT * FROM player_quest WHERE user_id = $user ORDER BY quest_key ASC",
            params!(user = user_id.to_string()),
        )
        .await
    }

    /// Projects a counter onto a quest, opening the progress row when the
    /// player has never been credited for it. A `claimed` row is never
    /// rewritten, so a reward already paid cannot be re-earned by an event.
    pub async fn project_player_quest(
        &self,
        user_id: Uuid,
        quest_key: &str,
        progress: i64,
        state: &str,
    ) -> Result<Option<PlayerQuest>, DbError> {
        self.opt(
            "UPSERT type::record('player_quest', $id) \
             SET user_id = $user, quest_key = $quest, progress = $progress, \
                 state = $state, update_time = $now \
             WHERE state != 'claimed' RETURN AFTER",
            params!(
                id = player_quest_id(user_id, quest_key),
                user = user_id.to_string(),
                quest = quest_key,
                progress = progress,
                state = state,
                now = ts(Utc::now()),
            ),
        )
        .await
    }

    /// Takes the reward for a finished quest exactly once.
    pub async fn claim_player_quest(
        &self,
        user_id: Uuid,
        quest_key: &str,
        goal_amount: i64,
    ) -> Result<Option<PlayerQuest>, DbError> {
        self.opt(
            "UPDATE type::record('player_quest', $id) \
             SET state = 'claimed', update_time = $now \
             WHERE progress >= $goal AND state != 'claimed' RETURN AFTER",
            params!(
                id = player_quest_id(user_id, quest_key),
                goal = goal_amount,
                now = ts(Utc::now()),
            ),
        )
        .await
    }

    /// Hands a quest back when a payout failed, so the player can retry it.
    pub async fn release_player_quest(
        &self,
        user_id: Uuid,
        quest_key: &str,
    ) -> Result<(), DbError> {
        self.exec(
            "UPDATE type::record('player_quest', $id) \
             SET state = 'completed', update_time = $now WHERE state = 'claimed'",
            params!(
                id = player_quest_id(user_id, quest_key),
                now = ts(Utc::now()),
            ),
        )
        .await
    }

    pub async fn list_player_stats(&self, user_id: Uuid) -> Result<Vec<PlayerStat>, DbError> {
        self.all(
            "SELECT * FROM player_stat WHERE user_id = $user ORDER BY stat ASC",
            params!(user = user_id.to_string()),
        )
        .await
    }

    pub async fn get_player_stat(
        &self,
        user_id: Uuid,
        stat: &str,
    ) -> Result<Option<PlayerStat>, DbError> {
        self.opt(
            "SELECT * FROM type::record('player_stat', $id) LIMIT 1",
            params!(id = player_stat_id(user_id, stat)),
        )
        .await
    }

    /// Adds to a counter, opening it at the delta when the player has no history
    /// for it, and returns the counter's new value.
    pub async fn increase_player_stat(
        &self,
        user_id: Uuid,
        stat: &str,
        delta: i64,
    ) -> Result<i64, DbError> {
        let id = player_stat_id(user_id, stat);
        for _ in 0..3 {
            let updated: Option<PlayerStat> = self
                .opt(
                    "UPDATE type::record('player_stat', $id) \
                     SET value = value + $delta, update_time = $now RETURN AFTER",
                    params!(id = id.clone(), delta = delta, now = ts(Utc::now())),
                )
                .await?;
            if let Some(row) = updated {
                return Ok(row.value);
            }
            let row = PlayerStat {
                id: id.clone(),
                user_id,
                stat: stat.to_string(),
                value: delta,
                update_time: Utc::now(),
            };
            if self
                .create("player_stat", Key::String(id.clone()), &row)
                .await
                .is_ok()
            {
                return Ok(delta);
            }
            // The row appeared between the update and the create, so the next
            // pass adds this delta onto what the other writer stored.
        }
        Ok(self
            .get_player_stat(user_id, stat)
            .await?
            .map(|row| row.value)
            .unwrap_or(delta))
    }

    /// Moves a marker to `day` only if that day has not been marked yet, which
    /// is what makes a second daily claim in the same day refuse.
    pub async fn mark_player_day(
        &self,
        user_id: Uuid,
        stat: &str,
        day: i64,
    ) -> Result<bool, DbError> {
        let id = player_stat_id(user_id, stat);
        let now = Utc::now();
        let row = PlayerStat {
            id: id.clone(),
            user_id,
            stat: stat.to_string(),
            value: day,
            update_time: now,
        };
        if self
            .create("player_stat", Key::String(id.clone()), &row)
            .await
            .is_ok()
        {
            return Ok(true);
        }
        let updated: Option<PlayerStat> = self
            .opt(
                "UPDATE type::record('player_stat', $id) \
                 SET value = $day, update_time = $now WHERE value < $day RETURN AFTER",
                params!(id = id, day = day, now = ts(now)),
            )
            .await?;
        Ok(updated.is_some())
    }

    /// Puts a marker back after a payout failed, so the day is claimable again.
    pub async fn rewind_player_stat(
        &self,
        user_id: Uuid,
        stat: &str,
        value: Option<i64>,
    ) -> Result<(), DbError> {
        let id = player_stat_id(user_id, stat);
        match value {
            Some(value) => {
                self.exec(
                    "UPDATE type::record('player_stat', $id) SET value = $value, \
                     update_time = $now",
                    params!(id = id, value = value, now = ts(Utc::now())),
                )
                .await
            }
            None => {
                self.exec("DELETE type::record('player_stat', $id)", params!(id = id))
                    .await
            }
        }
    }

    pub async fn setting_value(&self, name: &str) -> Result<Option<Value>, DbError> {
        self.opt(
            "SELECT VALUE value FROM type::record('setting', $name) LIMIT 1",
            params!(name = name),
        )
        .await
    }
}
