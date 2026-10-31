use super::*;

impl Queries {
    pub async fn list_game_maps(&self, game_id: Uuid) -> Result<Vec<ListGameMapsRow>, DbError> {
        self.all(
            &format!(
                "SELECT {GAME_MAP_COLUMNS} FROM game_maps \
                 WHERE game_id = $game_id ORDER BY version DESC"
            ),
            params!(game_id = game_id.to_string()),
        )
        .await
    }

    pub async fn latest_game_map_version(&self, game_id: Uuid) -> Result<Option<i32>, DbError> {
        self.opt(
            "SELECT VALUE version FROM game_maps WHERE game_id = $game_id \
             ORDER BY version DESC LIMIT 1",
            params!(game_id = game_id.to_string()),
        )
        .await
    }

    pub async fn game_map_at_version(
        &self,
        game_id: Uuid,
        version: i32,
    ) -> Result<Option<StoredGameMapRow>, DbError> {
        self.opt(
            &format!(
                "SELECT {GAME_MAP_COLUMNS}, blob_key FROM game_maps \
                 WHERE game_id = $game_id AND version = $version"
            ),
            params!(game_id = game_id.to_string(), version = version),
        )
        .await
    }

    pub async fn insert_game_map(
        &self,
        game_id: Uuid,
        id: Uuid,
        format: &str,
        data: &Value,
    ) -> Result<(), DbError> {
        self.exec(
            "UPDATE game_maps SET is_active = false \
             WHERE game_id = $game_id AND format = $format; \
             CREATE type::record('game_maps', <uuid>$id) CONTENT $data",
            params!(
                game_id = game_id.to_string(),
                format = format,
                id = id.to_string(),
                data = data,
            ),
        )
        .await
    }

    pub async fn activate_game_map(
        &self,
        game_id: Uuid,
        version: i32,
        format: &str,
    ) -> Result<(), DbError> {
        self.exec(
            "UPDATE game_maps SET is_active = false \
             WHERE game_id = $game_id AND format = $format; \
             UPDATE game_maps SET is_active = true \
             WHERE game_id = $game_id AND version = $version",
            params!(
                game_id = game_id.to_string(),
                format = format,
                version = version,
            ),
        )
        .await
    }

    pub async fn find_game_map_format(
        &self,
        game_id: Uuid,
        version: i32,
    ) -> Result<Vec<String>, DbError> {
        self.all(
            "SELECT VALUE format FROM game_maps \
             WHERE game_id = $game_id AND version = $version",
            params!(game_id = game_id.to_string(), version = version),
        )
        .await
    }

    pub async fn game_map_active_flag(
        &self,
        game_id: Uuid,
        version: i32,
    ) -> Result<Vec<bool>, DbError> {
        self.all(
            "SELECT VALUE is_active FROM game_maps \
             WHERE game_id = $game_id AND version = $version",
            params!(game_id = game_id.to_string(), version = version),
        )
        .await
    }

    pub async fn delete_game_map(
        &self,
        game_id: Uuid,
        version: i32,
    ) -> Result<Vec<DeletedGameMapRow>, DbError> {
        self.all(
            "DELETE game_maps WHERE game_id = $game_id AND version = $version RETURN BEFORE",
            params!(game_id = game_id.to_string(), version = version),
        )
        .await
    }

    pub async fn load_active_game_maps(
        &self,
        game_id: Uuid,
    ) -> Result<Vec<LoadActiveGameMapsRow>, DbError> {
        self.all(
            "SELECT name, format, version, blob_key FROM game_maps \
             WHERE game_id = $game_id AND is_active",
            params!(game_id = game_id.to_string()),
        )
        .await
    }
}
