use super::*;

impl Queries {
    pub(super) async fn game_account_ids(&self, game_id: Uuid) -> Result<Vec<Uuid>, DbError> {
        let rows: Vec<IdRow> = self
            .all(
                "SELECT id FROM users WHERE game_id = $game_id",
                params!(game_id = game_id.to_string()),
            )
            .await?;
        Ok(rows.into_iter().map(|row| row.id).collect())
    }

    pub async fn tenancy_game_by_id(&self, id: Uuid) -> Result<Vec<TenancyGameRow>, DbError> {
        self.all(
            &format!("SELECT {GAME_COLUMNS} FROM {}", record_uuid("games", "id")),
            params!(id = id.to_string()),
        )
        .await
    }

    pub async fn tenancy_game_by_key(
        &self,
        game_key: &str,
    ) -> Result<Vec<TenancyGameRow>, DbError> {
        self.all(
            &format!("SELECT {GAME_COLUMNS} FROM games WHERE game_key = $game_key"),
            params!(game_key = game_key),
        )
        .await
    }

    pub async fn tenancy_game_by_name_or_key(
        &self,
        name: &str,
    ) -> Result<Option<TenancyGameRow>, DbError> {
        self.opt(
            &format!(
                "SELECT {GAME_COLUMNS} FROM games \
                 WHERE (string::lowercase(name) = $needle OR string::lowercase(game_key) = $needle) \
                 AND status = 'active' ORDER BY created_at DESC LIMIT 1"
            ),
            params!(needle = name.to_lowercase()),
        )
        .await
    }

    pub async fn tenancy_sole_active_games(&self) -> Result<Vec<TenancyGameRow>, DbError> {
        self.all(
            &format!(
                "SELECT {GAME_COLUMNS} FROM games WHERE status = 'active' \
                 ORDER BY created_at ASC LIMIT 2"
            ),
            BTreeMap::new(),
        )
        .await
    }

    pub async fn backfill_users_game(&self, game_id: Uuid) -> Result<WriteResult, DbError> {
        affected(
            self,
            "UPDATE users SET game_id = $game_id \
             WHERE game_id IS NONE OR game_id IS NULL RETURN AFTER",
            params!(game_id = game_id.to_string()),
        )
        .await
    }

    pub async fn backfill_leaderboard_game(&self, game_id: Uuid) -> Result<WriteResult, DbError> {
        affected(
            self,
            "UPDATE leaderboard SET game_id = $game_id \
             WHERE game_id IS NONE OR game_id IS NULL RETURN AFTER",
            params!(game_id = game_id.to_string()),
        )
        .await
    }

    async fn backfill_account_rows(
        &self,
        table: &str,
        game_id: Uuid,
    ) -> Result<WriteResult, DbError> {
        let user_ids = self.game_account_ids(game_id).await?;
        affected(
            self,
            &format!(
                "UPDATE {table} SET game_id = $game_id \
                 WHERE (game_id IS NONE OR game_id IS NULL) AND user_id IN $user_ids \
                 RETURN AFTER"
            ),
            params!(game_id = game_id.to_string(), user_ids = user_ids,),
        )
        .await
    }

    pub async fn backfill_purchases_game(&self, game_id: Uuid) -> Result<WriteResult, DbError> {
        self.backfill_account_rows("purchase", game_id).await
    }

    pub async fn backfill_subscriptions_game(&self, game_id: Uuid) -> Result<WriteResult, DbError> {
        self.backfill_account_rows("subscription", game_id).await
    }

    pub async fn backfill_wallet_ledger_game(&self, game_id: Uuid) -> Result<WriteResult, DbError> {
        self.backfill_account_rows("wallet_ledger", game_id).await
    }

    pub async fn stamp_user_game(&self, game_id: Uuid, id: Uuid) -> Result<WriteResult, DbError> {
        affected(
            self,
            &format!(
                "UPDATE {} SET game_id = $game_id \
                 WHERE game_id IS NONE OR game_id IS NULL RETURN AFTER",
                record_uuid("users", "id")
            ),
            params!(game_id = game_id.to_string(), id = id.to_string(),),
        )
        .await
    }

    pub async fn tenancy_accounts(
        &self,
        game: Option<Uuid>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Users>, DbError> {
        match game {
            None => self.list_accounts(limit, offset).await,
            Some(game_id) => {
                self.all(
                    "SELECT * FROM users WHERE game_id = $game_id \
                     ORDER BY create_time DESC, id ASC LIMIT $limit START $offset",
                    params!(
                        game_id = game_id.to_string(),
                        limit = limit,
                        offset = offset,
                    ),
                )
                .await
            }
        }
    }

    pub async fn tenancy_groups(
        &self,
        game: Option<Uuid>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Groups>, DbError> {
        match game {
            None => {
                self.list_groups(&ListGroupsParams {
                    lang_tag: String::new(),
                    state: -1,
                    edge_count: -1,
                    limit,
                    offset,
                })
                .await
            }
            Some(game_id) => {
                let creator_ids = self.game_account_ids(game_id).await?;
                self.all(
                    "SELECT * FROM groups WHERE disable_time = $epoch \
                     AND creator_id IN $creator_ids \
                     ORDER BY update_time ASC, edge_count ASC, id ASC \
                     LIMIT $limit START $offset",
                    params!(
                        epoch = time::EPOCH,
                        creator_ids = creator_ids,
                        limit = limit,
                        offset = offset,
                    ),
                )
                .await
            }
        }
    }

    pub async fn tenancy_boards(
        &self,
        game: Option<Uuid>,
        category: i16,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Leaderboard>, DbError> {
        match game {
            None => {
                self.list_leaderboards_by_category(&ListLeaderboardsByCategoryParams {
                    category,
                    limit,
                    offset,
                })
                .await
            }
            Some(game_id) => {
                self.all(
                    "SELECT * FROM leaderboard WHERE category = $category \
                     AND game_id = $game_id \
                     ORDER BY create_time DESC, id ASC LIMIT $limit START $offset",
                    params!(
                        category = category,
                        game_id = game_id.to_string(),
                        limit = limit,
                        offset = offset,
                    ),
                )
                .await
            }
        }
    }

    pub async fn tenancy_storage(
        &self,
        game: Option<Uuid>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<TenancyStorageRow>, DbError> {
        match game {
            None => self.list_storage(limit, offset).await,
            Some(game_id) => {
                let user_ids = self.game_account_ids(game_id).await?;
                let rows: Vec<StoredObject> = self
                    .all(
                        &format!(
                            "SELECT {STORAGE_COLUMNS} FROM storage WHERE user_id IN $user_ids \
                             ORDER BY update_time DESC, collection ASC, id ASC \
                             LIMIT $limit START $offset"
                        ),
                        params!(user_ids = user_ids, limit = limit, offset = offset,),
                    )
                    .await?;
                Ok(rows.into_iter().map(ListStorageRow::from).collect())
            }
        }
    }

    pub async fn tenancy_storage_collections(
        &self,
        game: Option<Uuid>,
    ) -> Result<Vec<String>, DbError> {
        match game {
            None => self.list_storage_collections().await,
            Some(game_id) => {
                let user_ids = self.game_account_ids(game_id).await?;
                let rows: Vec<CollectionRow> = self
                    .all(
                        "SELECT collection FROM storage WHERE user_id IN $user_ids \
                         GROUP BY collection",
                        params!(user_ids = user_ids),
                    )
                    .await?;
                let mut collections: Vec<String> =
                    rows.into_iter().map(|row| row.collection).collect();
                collections.sort();
                Ok(collections)
            }
        }
    }

    pub async fn tenancy_storage_in_collection(
        &self,
        game: Option<Uuid>,
        collection: &str,
        limit: i64,
    ) -> Result<Vec<TenancyStorageRow>, DbError> {
        match game {
            None => self.list_storage_in_collection(collection, limit).await,
            Some(game_id) => {
                let user_ids = self.game_account_ids(game_id).await?;
                let rows: Vec<StoredObject> = self
                    .all(
                        &format!(
                            "SELECT {STORAGE_COLUMNS} FROM storage \
                             WHERE collection = $collection AND user_id IN $user_ids \
                             ORDER BY id ASC, user_id ASC LIMIT $limit"
                        ),
                        params!(collection = collection, user_ids = user_ids, limit = limit,),
                    )
                    .await?;
                Ok(rows.into_iter().map(ListStorageRow::from).collect())
            }
        }
    }

    pub async fn tenancy_purchases(
        &self,
        game: Option<Uuid>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Purchase>, DbError> {
        match game {
            None => self.list_all_purchases(limit, offset).await,
            Some(game_id) => {
                let user_ids = self.game_account_ids(game_id).await?;
                self.all(
                    "SELECT * FROM purchase WHERE user_id IN $user_ids \
                     ORDER BY purchase_time DESC, transaction_id ASC \
                     LIMIT $limit START $offset",
                    params!(user_ids = user_ids, limit = limit, offset = offset,),
                )
                .await
            }
        }
    }

    pub async fn tenancy_subscriptions(
        &self,
        game: Option<Uuid>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Subscription>, DbError> {
        match game {
            None => self.list_all_subscriptions(limit, offset).await,
            Some(game_id) => {
                let user_ids = self.game_account_ids(game_id).await?;
                self.all(
                    "SELECT * FROM subscription WHERE user_id IN $user_ids \
                     ORDER BY purchase_time DESC, original_transaction_id ASC \
                     LIMIT $limit START $offset",
                    params!(user_ids = user_ids, limit = limit, offset = offset,),
                )
                .await
            }
        }
    }

    pub async fn tenancy_account_in_game(
        &self,
        user_id: Uuid,
        game_id: Uuid,
    ) -> Result<bool, DbError> {
        let row: Option<IdRow> = self
            .opt(
                "SELECT id FROM type::record('users', <uuid>$user_id) \
                 WHERE game_id = $game",
                params!(user_id = user_id.to_string(), game = game_id.to_string(),),
            )
            .await?;
        Ok(row.is_some())
    }

    pub async fn tenancy_group_in_game(
        &self,
        group_id: Uuid,
        game_id: Uuid,
    ) -> Result<bool, DbError> {
        let creator: Option<CreatorRow> = self
            .opt(
                "SELECT creator_id FROM type::record('groups', <uuid>$group_id)",
                params!(group_id = group_id.to_string()),
            )
            .await?;
        let Some(creator) = creator else {
            return Ok(false);
        };
        let owned: Option<IdRow> = self
            .opt(
                "SELECT id FROM type::record('users', <uuid>$creator_id) \
                 WHERE game_id = $game",
                params!(
                    creator_id = creator.creator_id.to_string(),
                    game = game_id.to_string(),
                ),
            )
            .await?;
        Ok(owned.is_some())
    }

    pub async fn tenancy_board_in_game(
        &self,
        board_id: &str,
        game_id: Uuid,
    ) -> Result<bool, DbError> {
        let row: Option<Value> = self
            .opt(
                "SELECT VALUE id FROM type::record('leaderboard', $board_id) \
                 WHERE game_id = $game",
                params!(board_id = board_id, game = game_id.to_string(),),
            )
            .await?;
        Ok(row.is_some())
    }

    pub async fn tenancy_delete_notification(
        &self,
        game: Option<Uuid>,
        id: Uuid,
    ) -> Result<u64, DbError> {
        let rows: Vec<Value> = match game {
            None => {
                self.all(
                    "DELETE type::record('notification', <uuid>$id) RETURN BEFORE",
                    params!(id = id.to_string()),
                )
                .await?
            }
            Some(game_id) => {
                let user_ids = self.game_account_ids(game_id).await?;
                self.all(
                    "DELETE notification WHERE id = type::record('notification', <uuid>$id) \
                     AND user_id IN $user_ids RETURN BEFORE",
                    params!(id = id.to_string(), user_ids = user_ids,),
                )
                .await?
            }
        };
        Ok(rows.len() as u64)
    }

    pub async fn tenancy_delete_subscriptions_for_product(
        &self,
        game: Option<Uuid>,
        product_id: &str,
    ) -> Result<u64, DbError> {
        let rows: Vec<Value> = match game {
            None => {
                self.all(
                    "DELETE subscription WHERE product_id = $product_id RETURN BEFORE",
                    params!(product_id = product_id),
                )
                .await?
            }
            Some(game_id) => {
                let user_ids = self.game_account_ids(game_id).await?;
                self.all(
                    "DELETE subscription WHERE product_id = $product_id \
                     AND user_id IN $user_ids RETURN BEFORE",
                    params!(product_id = product_id, user_ids = user_ids,),
                )
                .await?
            }
        };
        Ok(rows.len() as u64)
    }

    pub async fn tenancy_delete_storage_collection(
        &self,
        game: Option<Uuid>,
        collection: &str,
    ) -> Result<u64, DbError> {
        let rows: Vec<Value> = match game {
            None => {
                self.all(
                    "DELETE storage WHERE collection = $collection RETURN BEFORE",
                    params!(collection = collection),
                )
                .await?
            }
            Some(game_id) => {
                let user_ids = self.game_account_ids(game_id).await?;
                self.all(
                    "DELETE storage WHERE collection = $collection \
                     AND user_id IN $user_ids RETURN BEFORE",
                    params!(collection = collection, user_ids = user_ids,),
                )
                .await?
            }
        };
        Ok(rows.len() as u64)
    }
}
