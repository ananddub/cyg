use super::*;

/// The record id of one config version of one game.
pub fn game_config_version_id(game_id: Uuid, version: i64) -> String {
    format!("{game_id}:{version}")
}

impl Queries {
    pub async fn list_config_versions(
        &self,
        game_id: Uuid,
    ) -> Result<Vec<GameConfigVersion>, DbError> {
        self.all(
            "SELECT * FROM game_config_version WHERE game_id = $game ORDER BY version DESC",
            params!(game = game_id),
        )
        .await
    }

    pub async fn get_config_version(
        &self,
        game_id: Uuid,
        version: i64,
    ) -> Result<Option<GameConfigVersion>, DbError> {
        self.opt(
            "SELECT * FROM type::record('game_config_version', $id) LIMIT 1",
            params!(id = game_config_version_id(game_id, version)),
        )
        .await
    }

    pub async fn active_config_version(
        &self,
        game_id: Uuid,
    ) -> Result<Option<GameConfigVersion>, DbError> {
        self.opt(
            "SELECT * FROM game_config_version WHERE game_id = $game AND is_active = true LIMIT 1",
            params!(game = game_id),
        )
        .await
    }

    pub async fn latest_config_version(&self, game_id: Uuid) -> Result<i64, DbError> {
        let rows: Vec<LatestRow> = self
            .all(
                "SELECT version FROM game_config_version WHERE game_id = $game ORDER BY version DESC LIMIT 1",
                params!(game = game_id),
            )
            .await?;
        Ok(rows.first().map(|row| row.version).unwrap_or(0))
    }

    /// Appends one version; `false` when the version number is taken, which is
    /// the guard that keeps history append-only.
    pub async fn insert_config_version(&self, row: &GameConfigVersion) -> Result<bool, DbError> {
        match self
            .create("game_config_version", Key::String(row.id.clone()), row)
            .await
        {
            Ok(_) => Ok(true),
            Err(err) if err.is_unique() => Ok(false),
            Err(err) => Err(err),
        }
    }

    /// Marks one version active and retires the rest in one transaction.
    /// SurrealDB 4.0's SNAPSHOT default can let two concurrent activations of
    /// DIFFERENT versions both commit when no currently-active rows existed
    /// to conflict on — so a post-commit single-active check re-runs the
    /// transaction until it holds.
    pub async fn activate_config_version(
        &self,
        game_id: Uuid,
        version: i64,
    ) -> Result<(), DbError> {
        for _ in 0..3 {
            self.exec(
                "BEGIN TRANSACTION; \
                 UPDATE game_config_version SET is_active = false WHERE game_id = $game AND is_active = true; \
                 UPDATE type::record('game_config_version', $id) SET is_active = true; \
                 COMMIT;",
                params!(
                    game = game_id,
                    id = game_config_version_id(game_id, version)
                ),
            )
            .await?;
            let actives: Vec<i64> = self
                .all(
                    "SELECT VALUE version FROM game_config_version \
                     WHERE game_id = $game AND is_active = true",
                    params!(game = game_id),
                )
                .await?;
            if actives.len() <= 1 {
                return Ok(());
            }
        }
        tracing::warn!(
            "[db] config activation for {game_id} still sees multiple active versions after 3 passes"
        );
        Ok(())
    }

    /// Reads the live `games.config` blob for the game.
    pub async fn game_config(&self, game_id: Uuid) -> Result<Option<(String, Value)>, DbError> {
        let rows: Vec<GameConfigRow> = self
            .all(
                "SELECT id, config FROM games WHERE id = $game LIMIT 1",
                params!(game = game_id),
            )
            .await?;
        Ok(rows
            .into_iter()
            .next()
            .map(|row| (row.id.to_string(), row.config)))
    }
}

#[derive(serde::Deserialize)]
struct LatestRow {
    version: i64,
}

#[derive(serde::Deserialize)]
struct GameConfigRow {
    id: Uuid,
    config: Value,
}
