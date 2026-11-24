use super::*;

impl GamesService {
    pub async fn archive_game(&self, game_id: Uuid) -> Result<bool, V2Error> {
        Self::find_game(self, game_id).await?;
        let done = self
            .plane
            .q()
            .await?
            .archive_game(game_id)
            .await
            .map_err(V2Error::from)?;
        Ok(done.rows_affected() > 0)
    }
    /// Batch read (`API Design Patterns`, ch. 18): every id must exist, and the
    /// reply keeps the caller's order.
    pub async fn batch_get(&self, ids: &[Uuid]) -> Result<Vec<GameDto>, V2Error> {
        if ids.is_empty() {
            return Err(V2Error::invalid_argument(
                "A batch request must name at least one game.",
            ));
        }
        if ids.len() > 100 {
            return Err(V2Error::invalid_argument(
                "A batch request names at most 100 games.",
            ));
        }
        let mut games = Vec::with_capacity(ids.len());
        for id in ids {
            games.push(self.find_game(*id).await.map(GameDto::from)?);
        }
        Ok(games)
    }

    /// Batch archive. Missing ids are refused before anything is written, so a
    /// typo cannot leave a partially archived batch.
    pub async fn batch_archive(&self, ids: &[Uuid]) -> Result<usize, V2Error> {
        if ids.is_empty() {
            return Err(V2Error::invalid_argument(
                "A batch request must name at least one game.",
            ));
        }
        if ids.len() > 100 {
            return Err(V2Error::invalid_argument(
                "A batch request names at most 100 games.",
            ));
        }
        let mut unique = Vec::new();
        for id in ids {
            if !unique.contains(id) {
                unique.push(*id);
            }
        }
        for id in &unique {
            Self::find_game(self, *id).await?;
        }
        for id in &unique {
            self.archive_game(*id).await?;
        }
        Ok(unique.len())
    }

    /// Criteria-based archive (`API Design Patterns`, ch. 19). Empty criteria
    /// are refused so a missing filter cannot archive the fleet.
    pub async fn archive_matching(
        &self,
        status: Option<&str>,
        name_contains: Option<&str>,
    ) -> Result<usize, V2Error> {
        let status = status.map(game_status).transpose()?;
        let name_contains = name_contains
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if let Some(value) = name_contains {
            if value.len() > 128 {
                return Err(V2Error::invalid_argument(
                    "A name filter is at most 128 characters.",
                ));
            }
        }
        if status.is_none() && name_contains.is_none() {
            return Err(V2Error::invalid_argument(
                "A criteria delete needs at least one filter.",
            ));
        }
        let done = self
            .plane
            .q()
            .await?
            .archive_games_matching(status.as_deref(), name_contains)
            .await
            .map_err(V2Error::from)?;
        Ok(done.rows_affected() as usize)
    }

    /// Soft-delete restore (`API Design Patterns`, ch. 25). The row and its
    /// tenant key survive an archive; undelete only moves the state back.
    pub async fn restore_game(&self, game_id: Uuid) -> Result<GameDto, V2Error> {
        Self::find_game(self, game_id).await?;
        let done = self
            .plane
            .q()
            .await?
            .restore_game(game_id)
            .await
            .map_err(V2Error::from)?;
        if done.rows_affected() == 0 {
            return Err(V2Error::failed_precondition(
                "This game is not archived, so there is nothing to undelete.",
            ));
        }
        self.find_game(game_id).await.map(GameDto::from)
    }
}
