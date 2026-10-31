use super::*;

impl Queries {
    pub async fn delete_room_module(
        &self,
        game_id: Uuid,
        version: i32,
    ) -> Result<Vec<DeletedRoomModuleRow>, DbError> {
        self.all(
            "DELETE room_modules WHERE game_id = $game_id AND version = $version RETURN BEFORE",
            params!(game_id = game_id.to_string(), version = version),
        )
        .await
    }

    pub async fn set_game_backend_mode(
        &self,
        game_id: Uuid,
        backend_mode: &str,
    ) -> Result<(), DbError> {
        self.exec(
            "UPDATE type::record('games', <uuid>$id) SET backend_mode = $backend_mode RETURN NOTHING",
            params!(
                id = game_id.to_string(),
                backend_mode = backend_mode,
            ),
        )
        .await
    }

    pub async fn latest_room_module_version(
        &self,
        game_id: Uuid,
    ) -> Result<Option<i32>, DbError> {
        self.opt(
            "SELECT VALUE version FROM room_modules WHERE game_id = $game_id \
             ORDER BY version DESC LIMIT 1",
            params!(game_id = game_id.to_string()),
        )
        .await
    }

    pub async fn room_module_at_version(
        &self,
        game_id: Uuid,
        version: i32,
    ) -> Result<Option<ListRoomModulesRow>, DbError> {
        self.opt(
            &format!(
                "SELECT {ROOM_MODULE_COLUMNS} FROM room_modules \
                 WHERE game_id = $game_id AND version = $version"
            ),
            params!(game_id = game_id.to_string(), version = version),
        )
        .await
    }

    pub async fn insert_room_module(
        &self,
        game_id: Uuid,
        id: Uuid,
        data: &Value,
    ) -> Result<(), DbError> {
        self.exec(
            "UPDATE room_modules SET is_active = false \
             WHERE game_id = $game_id AND is_active; \
             CREATE type::record('room_modules', <uuid>$id) CONTENT $data",
            params!(
                game_id = game_id.to_string(),
                id = id.to_string(),
                data = data,
            ),
        )
        .await
    }

    pub async fn activate_room_module(&self, game_id: Uuid, version: i32) -> Result<(), DbError> {
        self.exec(
            "UPDATE room_modules SET is_active = false \
             WHERE game_id = $game_id AND is_active; \
             UPDATE room_modules SET is_active = true \
             WHERE game_id = $game_id AND version = $version",
            params!(
                game_id = game_id.to_string(),
                version = version,
            ),
        )
        .await
    }
}

impl Queries {
    pub async fn insert_game(&self, params: &InsertGameParams) -> Result<InsertGameRow, DbError> {
        let id = Uuid::new_v4();
        let row = new_game(id, params);
        let stored = self.create("games", Key::Uuid(id), &row).await?;
        Ok(InsertGameRow::from(stored))
    }
    pub async fn list_games(&self) -> Result<Vec<ListGamesRow>, DbError> {
        self.all(
            &format!("SELECT {GAME_COLUMNS} FROM games ORDER BY created_at DESC"),
            BTreeMap::new(),
        )
        .await
    }
    pub async fn count_games(&self) -> Result<i64, DbError> {
        Ok(self
            .opt::<i64>(
                "RETURN array::len((SELECT id FROM games))",
                BTreeMap::new(),
            )
            .await?
            .unwrap_or(0))
    }
    pub async fn find_game_by_id(&self, id: Uuid) -> Result<Vec<FindGameByIdRow>, DbError> {
        self.all(
            &format!(
                "SELECT {GAME_COLUMNS} FROM {}",
                record_uuid("games", "id")
            ),
            params!(id = id.to_string()),
        )
        .await
    }
    pub async fn find_game_by_key(
        &self,
        game_key: &str,
    ) -> Result<Vec<FindGameByKeyRow>, DbError> {
        self.all(
            &format!("SELECT {GAME_COLUMNS} FROM games WHERE game_key = $game_key"),
            params!(game_key = game_key),
        )
        .await
    }
    pub async fn find_game_secret_by_id(&self, id: Uuid) -> Result<Vec<String>, DbError> {
        self.all(
            &format!(
                "SELECT VALUE api_key FROM {}",
                record_uuid("games", "id")
            ),
            params!(id = id.to_string()),
        )
        .await
    }
    pub async fn find_game_secret_by_key(&self, game_key: &str) -> Result<Vec<String>, DbError> {
        self.all(
            "SELECT VALUE api_key FROM games WHERE game_key = $game_key",
            params!(game_key = game_key),
        )
        .await
    }
    pub async fn update_game(&self, params: &UpdateGameParams) -> Result<UpdateGameRow, DbError> {
        self.one(
            &format!(
                "UPDATE {} SET name = $name, engine_type = $engine_type, status = $status, \
                 config = $config, backend_mode = $backend_mode, updated_at = $updated_at",
                record_uuid("games", "id")
            ),
            params!(
                name = params.name,
                engine_type = params.engine_type,
                status = params.status,
                config = params.config,
                backend_mode = params.backend_mode,
                updated_at = time::encode(Utc::now()),
                id = params.id.to_string(),
            ),
        )
        .await
    }
    pub async fn archive_game(&self, id: Uuid) -> Result<WriteResult, DbError> {
        affected(
            self,
            &format!(
                "UPDATE {} SET status = 'archived', updated_at = $updated_at RETURN AFTER",
                record_uuid("games", "id")
            ),
            params!(
                updated_at = time::encode(Utc::now()),
                id = id.to_string(),
            ),
        )
        .await
    }
    pub async fn active_room_module_version(&self, game_id: Uuid) -> Result<Vec<i32>, DbError> {
        self.all(
            "SELECT VALUE version FROM room_modules \
             WHERE game_id = $game_id AND is_active \
             ORDER BY version DESC LIMIT 1",
            params!(game_id = game_id.to_string()),
        )
        .await
    }
    pub async fn count_room_modules(&self, game_id: Uuid) -> Result<i64, DbError> {
        Ok(self
            .opt::<i64>(
                "RETURN array::len((SELECT id FROM room_modules \
                 WHERE game_id = $game_id))",
                params!(game_id = game_id.to_string()),
            )
            .await?
            .unwrap_or(0))
    }
    pub async fn list_room_modules(
        &self,
        game_id: Uuid,
    ) -> Result<Vec<ListRoomModulesRow>, DbError> {
        self.all(
            &format!(
                "SELECT {ROOM_MODULE_COLUMNS} FROM room_modules \
                 WHERE game_id = $game_id ORDER BY version DESC"
            ),
            params!(game_id = game_id.to_string()),
        )
        .await
    }
    pub async fn find_room_module_bytes(
        &self,
        game_id: Uuid,
        version: i32,
    ) -> Result<Vec<FindRoomModuleBytesRow>, DbError> {
        self.all(
            "SELECT filename, blob_key FROM room_modules \
             WHERE game_id = $game_id AND version = $version",
            params!(game_id = game_id.to_string(), version = version),
        )
        .await
    }
    pub async fn room_module_active_flag(
        &self,
        game_id: Uuid,
        version: i32,
    ) -> Result<Vec<bool>, DbError> {
        self.all(
            "SELECT VALUE is_active FROM room_modules \
             WHERE game_id = $game_id AND version = $version",
            params!(game_id = game_id.to_string(), version = version),
        )
        .await
    }
}
