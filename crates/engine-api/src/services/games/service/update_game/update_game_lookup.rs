use super::*;

impl GamesService {
    pub async fn find_game(&self, game_id: Uuid) -> Result<GameRow, V2Error> {
        self.plane
            .q()
            .await?
            .find_game_by_id(game_id)
            .await
            .map_err(V2Error::from)?
            .into_iter()
            .next()
            .map(GameRow::from)
            .ok_or_else(|| V2Error::not_found(NOT_FOUND))
    }
    pub async fn find_game_by_key(&self, game_key: &str) -> Result<Option<GameRow>, V2Error> {
        Ok(self
            .plane
            .q()
            .await?
            .find_game_by_key(game_key)
            .await
            .map_err(V2Error::from)?
            .into_iter()
            .next()
            .map(GameRow::from))
    }
    pub async fn verify_api_key(&self, game_key: &str, api_key: &str) -> Result<bool, V2Error> {
        let Some(stored) = self
            .plane
            .q()
            .await?
            .find_game_secret_by_key(game_key)
            .await
            .map_err(V2Error::from)?
            .into_iter()
            .next()
        else {
            return Ok(false);
        };
        Ok(secrets_match(stored.as_bytes(), api_key.as_bytes()))
    }
    pub async fn active_module_version(&self, game_id: Uuid) -> Result<Option<i32>, V2Error> {
        Ok(self
            .plane
            .q()
            .await?
            .active_room_module_version(game_id)
            .await
            .map_err(V2Error::from)?
            .into_iter()
            .next())
    }
    pub async fn module_count(&self, game_id: Uuid) -> Result<i64, V2Error> {
        self.plane
            .q()
            .await?
            .count_room_modules(game_id)
            .await
            .map_err(V2Error::from)
    }
}
