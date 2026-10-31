use super::*;

impl Queries {
    pub async fn find_console_user(
        &self,
        username: &str,
    ) -> Result<FindConsoleUserRow, DbError> {
        self.one(
            "SELECT id, username, email, acl, metadata, create_time, update_time, disable_time, mfa_required \
             FROM console_user WHERE username = $username OR email = $username LIMIT 1",
            params!(username = username),
        )
        .await
    }
    pub async fn list_console_users(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ListConsoleUsersRow>, DbError> {
        self.all(
            "SELECT id, username, email, acl, metadata, create_time, update_time, disable_time, mfa_required \
             FROM console_user ORDER BY create_time DESC, id ASC LIMIT $limit START $offset",
            params!(limit = limit, offset = offset),
        )
        .await
    }
    pub async fn list_accounts(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Users>, DbError> {
        self.all(
            "SELECT * FROM users ORDER BY create_time DESC, id ASC LIMIT $limit START $offset",
            params!(limit = limit, offset = offset),
        )
        .await
    }
    pub async fn count_accounts(
        &self,
    ) -> Result<i64, DbError> {
        Ok(self
            .opt::<i64>(
                "RETURN array::len((SELECT id FROM users))",
                std::collections::BTreeMap::new(),
            )
            .await?
            .unwrap_or(0))
    }
    pub async fn list_wallet_ledger(
        &self,
        params_in: &ListWalletLedgerParams,
    ) -> Result<Vec<WalletLedger>, DbError> {
        self.all(
            "SELECT id, user_id, changeset, metadata, create_time, update_time FROM wallet_ledger \
             WHERE user_id = $user_id ORDER BY create_time DESC, id ASC LIMIT $limit START $offset",
            params!(
                user_id = params_in.user_id.to_string(),
                limit = params_in.limit,
                offset = params_in.offset,
            ),
        )
        .await
    }
    pub async fn list_user_notes(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<UsersNotes>, DbError> {
        self.all(
            "SELECT id, user_id, create_time, update_time, note, create_id, update_id FROM users_notes \
             WHERE user_id = $user_id ORDER BY create_time DESC",
            params!(user_id = user_id.to_string()),
        )
        .await
    }
    pub async fn list_leaderboards(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Leaderboard>, DbError> {
        self.all(
            "SELECT * FROM leaderboard ORDER BY create_time DESC, id ASC LIMIT $limit START $offset",
            params!(limit = limit, offset = offset),
        )
        .await
    }
    pub async fn list_leaderboards_by_category(
        &self,
        params_in: &ListLeaderboardsByCategoryParams,
    ) -> Result<Vec<Leaderboard>, DbError> {
        self.all(
            "SELECT * FROM leaderboard WHERE category = $category \
             ORDER BY create_time DESC, id ASC LIMIT $limit START $offset",
            params!(
                category = params_in.category,
                limit = params_in.limit,
                offset = params_in.offset,
            ),
        )
        .await
    }
    pub async fn list_all_leaderboard_records(
        &self,
        leaderboard_id: &str,
        limit: i64,
    ) -> Result<Vec<LeaderboardRecord>, DbError> {
        self.all(
            "SELECT leaderboard_id, owner_id, username, score, subscore, num_score, metadata, \
                    create_time, update_time, expiry_time, max_num_score \
             FROM leaderboard_record WHERE leaderboard_id = $leaderboard_id \
             ORDER BY score DESC, subscore DESC, owner_id DESC LIMIT $limit",
            params!(leaderboard_id = leaderboard_id, limit = limit),
        )
        .await
    }
    pub async fn list_storage(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ListStorageRow>, DbError> {
        let rows: Vec<StoredObject> = self
            .all(
                &format!(
                    "SELECT {STORAGE_COLUMNS} FROM storage \
                     ORDER BY update_time DESC, collection ASC, id ASC LIMIT $limit START $offset"
                ),
                params!(limit = limit, offset = offset),
            )
            .await?;
        Ok(rows.into_iter().map(ListStorageRow::from).collect())
    }
    pub async fn list_storage_in_collection(
        &self,
        collection: &str,
        limit: i64,
    ) -> Result<Vec<ListStorageInCollectionRow>, DbError> {
        let rows: Vec<StoredObject> = self
            .all(
                &format!(
                    "SELECT {STORAGE_COLUMNS} FROM storage WHERE collection = $collection \
                     ORDER BY id ASC, user_id ASC LIMIT $limit"
                ),
                params!(collection = collection, limit = limit),
            )
            .await?;
        Ok(rows.into_iter().map(ListStorageRow::from).collect())
    }
}

impl Queries {
    pub async fn list_storage_collections(
        &self,
    ) -> Result<Vec<String>, DbError> {
        #[derive(serde::Deserialize)]
        pub(super) struct CollectionRow {
            pub(super) collection: String,
        }
        let rows: Vec<CollectionRow> = self
            .all(
                "SELECT collection FROM storage GROUP BY collection",
                std::collections::BTreeMap::new(),
            )
            .await?;
        let mut collections: Vec<String> = rows.into_iter().map(|row| row.collection).collect();
        collections.sort();
        Ok(collections)
    }
    pub async fn list_settings(
        &self,
    ) -> Result<Vec<Setting>, DbError> {
        self.all(
            "SELECT name, value, update_time FROM setting ORDER BY name ASC",
            std::collections::BTreeMap::new(),
        )
        .await
    }
    pub async fn list_audit_logs(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ConsoleAuditLog>, DbError> {
        self.all(
            "SELECT create_time, id, console_user_id, console_username, email, action, resource, \
                    message, metadata FROM console_audit_log \
             ORDER BY create_time DESC, id ASC LIMIT $limit START $offset",
            params!(limit = limit, offset = offset),
        )
        .await
    }
    pub async fn audit_usernames(&self) -> Result<Vec<String>, DbError> {
        let mut rows: Vec<AuditUsernameCount> = self
            .all(
                "SELECT console_username, count() AS total FROM console_audit_log \
                 GROUP BY console_username",
                std::collections::BTreeMap::new(),
            )
            .await?;
        rows.sort_by(|a, b| {
            b.total
                .cmp(&a.total)
                .then_with(|| a.console_username.cmp(&b.console_username))
        });
        Ok(rows.into_iter().map(|row| row.console_username).collect())
    }
    pub async fn list_acl_templates(
        &self,
    ) -> Result<Vec<ConsoleAclTemplate>, DbError> {
        self.all(
            "SELECT id, name, description, acl, create_time, update_time FROM console_acl_template \
             ORDER BY name ASC",
            std::collections::BTreeMap::new(),
        )
        .await
    }
    pub async fn list_all_purchases(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Purchase>, DbError> {
        self.all(
            "SELECT create_time, environment, product_id, purchase_time, raw_response, store, \
                    transaction_id, update_time, user_id, refund_time FROM purchase \
             ORDER BY purchase_time DESC, transaction_id ASC LIMIT $limit START $offset",
            params!(limit = limit, offset = offset),
        )
        .await
    }
    pub async fn list_notifications_for_user(
        &self,
        user_id: Uuid,
        limit: i64,
    ) -> Result<Vec<Notification>, DbError> {
        self.all(
            "SELECT id, user_id, subject, content, code, sender_id, create_time FROM notification \
             WHERE user_id = $user_id ORDER BY create_time DESC LIMIT $limit",
            params!(user_id = user_id.to_string(), limit = limit),
        )
        .await
    }
}
