use super::*;

impl GamesService {
    pub async fn create_game(
        state: &ApiState,
        body: &CreateGameRequest,
    ) -> Result<GameDto, V2Error> {
        let name = game_name(&body.name)?;
        let engine = engine_type(body.engine_type.as_deref().unwrap_or("unity"))?;
        let backend = backend_mode_for_engine(&engine);
        let config = create_config(body)?;

        let queries = q(state).await?;
        for _ in 0..KEY_ATTEMPTS {
            match queries
                .insert_game(&InsertGameParams {
                    game_key: game_key(&name),
                    name: name.clone(),
                    api_key: api_key(),
                    client_key: client_key(),
                    engine_type: engine.clone(),
                    backend_mode: backend.to_owned(),
                    config: config.clone(),
                })
                .await
            {
                Ok(row) => return Ok(GameDto::from(GameRow::from(row))),
                Err(err) if V2Error::is_any_unique_violation(&err) => continue,
                Err(err) => return Err(V2Error::from(err)),
            }
        }
        Err(V2Error::already_exists(
            "This game's name keeps colliding with an existing key; register it again.",
        ))
    }
    pub async fn list_games(state: &ApiState) -> Result<GameListDto, V2Error> {
        let queries = q(state).await?;
        let rows: Vec<ListGamesRow> = queries.list_games().await.map_err(V2Error::from)?;
        let total = queries.count_games().await.map_err(V2Error::from)?;
        Ok(GameListDto {
            games: rows
                .into_iter()
                .map(|row| GameDto::from(GameRow::from(row)))
                .collect(),
            total,
        })
    }
    pub async fn get_game(state: &ApiState, game_id: Uuid) -> Result<GameDetailDto, V2Error> {
        let row = Self::find_game(state, game_id).await?;
        let queries = q(state).await?;
        let api_key = queries
            .find_game_secret_by_id(game_id)
            .await
            .map_err(V2Error::from)?
            .into_iter()
            .next()
            .ok_or_else(|| V2Error::not_found(NOT_FOUND))?;
        Ok(GameDetailDto {
            id: row.id,
            game_key: row.game_key,
            name: row.name,
            client_key: row.client_key,
            engine_type: row.engine_type,
            backend_mode: row.backend_mode,
            status: row.status,
            config: row.config,
            created_at: row.created_at,
            updated_at: row.updated_at,
            api_key,
            active_module_version: Self::active_module_version(state, game_id).await?,
            module_count: Self::module_count(state, game_id).await?,
        })
    }
    pub async fn unity_settings(
        state: &ApiState,
        game_id: Uuid,
        reveal: bool,
        headers: &HeaderMap,
    ) -> Result<UnityConnectionDto, V2Error> {
        let detail = Self::get_game(state, game_id).await?;
        let config =
            serde_json::from_value::<GameConfig>(detail.config.clone()).unwrap_or_default();
        Ok(UnityConnectionDto {
            game_key: detail.game_key,
            client_key: detail.client_key,
            api_key: reveal.then_some(detail.api_key),
            server_http_url: advertised_base_url(state, headers),
            server_ws_path: SERVER_WS_PATH.to_owned(),
            default_room: DEFAULT_ROOM_NAME.to_owned(),
            tick_rate: config.tick_rate,
            max_players: config.max_clients.clamp(1, 255),
            persistent_rooms: config.persistent_rooms,
            backend_mode: detail.backend_mode,
        })
    }
}
