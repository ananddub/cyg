use super::*;

impl Queries {
    pub(super) async fn account_game_id(&self, user_id: Uuid) -> Result<Option<Uuid>, DbError> {
        let rows: Vec<Option<Uuid>> = self
            .all(
                "SELECT VALUE game_id FROM type::record('users', <uuid>$user_id) LIMIT 1",
                params!(user_id = user_id.to_string()),
            )
            .await?;
        Ok(rows.into_iter().flatten().next())
    }
    pub async fn insert_purchase(&self, p: &InsertPurchaseParams) -> Result<Purchase, DbError> {
        let now = Utc::now();
        let mut row = match self
            .opt::<Purchase>(
                "SELECT * FROM type::record('purchase', $transaction_id) LIMIT 1",
                params!(transaction_id = &p.transaction_id),
            )
            .await?
        {
            Some(row) => row,
            None => new_purchase(p, now),
        };
        row.user_id = p.user_id;
        row.purchase_time = p.purchase_time;
        row.refund_time = p.refund_time;
        row.raw_response = p.raw_response.clone();
        row.update_time = now;
        row.game_id = self.account_game_id(row.user_id).await?;

        self.one(
            "UPSERT type::record('purchase', $transaction_id) CONTENT $data RETURN AFTER",
            params!(
                transaction_id = &p.transaction_id,
                data = serde_json::to_value(&row)?,
            ),
        )
        .await
    }
    pub async fn get_purchase(&self, transaction_id: &str) -> Result<Purchase, DbError> {
        self.one(
            "SELECT * FROM type::record('purchase', $transaction_id) LIMIT 1",
            params!(transaction_id = transaction_id),
        )
        .await
    }
    pub async fn list_purchases(&self, p: &ListPurchasesParams) -> Result<Vec<Purchase>, DbError> {
        self.all(
            "SELECT * FROM purchase WHERE user_id = $user_id AND refund_time = $never \
             ORDER BY purchase_time DESC, transaction_id ASC LIMIT $limit START $offset",
            params!(
                user_id = p.user_id.to_string(),
                never = ts(time::epoch()),
                limit = p.limit,
                offset = p.offset,
            ),
        )
        .await
    }
    pub async fn insert_subscription(
        &self,
        p: &InsertSubscriptionParams,
    ) -> Result<Subscription, DbError> {
        let now = Utc::now();
        let mut row = match self
            .opt::<Subscription>(
                "SELECT * FROM type::record('subscription', $original_transaction_id) LIMIT 1",
                params!(original_transaction_id = &p.original_transaction_id),
            )
            .await?
        {
            Some(row) => row,
            None => new_subscription(p, now),
        };
        row.user_id = p.user_id;
        row.product_id = p.product_id.clone();
        row.purchase_time = p.purchase_time;
        row.expire_time = p.expire_time;
        row.refund_time = p.refund_time;
        row.raw_response = p.raw_response.clone();
        row.raw_notification = serde_json::json!({});
        row.update_time = now;
        row.game_id = self.account_game_id(row.user_id).await?;

        self.one(
            "UPSERT type::record('subscription', $original_transaction_id) CONTENT $data RETURN AFTER",
            params!(
                original_transaction_id = &p.original_transaction_id,
                data = serde_json::to_value(&row)?,
            ),
        )
        .await
    }
    pub async fn get_subscription(
        &self,
        original_transaction_id: &str,
    ) -> Result<Subscription, DbError> {
        self.one(
            "SELECT * FROM type::record('subscription', $original_transaction_id) LIMIT 1",
            params!(original_transaction_id = original_transaction_id),
        )
        .await
    }
    pub async fn list_subscriptions(
        &self,
        p: &ListSubscriptionsParams,
    ) -> Result<Vec<Subscription>, DbError> {
        self.all(
            "SELECT * FROM subscription WHERE user_id = $user_id \
               AND (expire_time > $now OR refund_time > $never) \
             ORDER BY purchase_time DESC, original_transaction_id ASC LIMIT $limit START $offset",
            params!(
                user_id = p.user_id.to_string(),
                now = ts(Utc::now()),
                never = ts(time::epoch()),
                limit = p.limit,
                offset = p.offset,
            ),
        )
        .await
    }
    pub async fn subscription_by_product(
        &self,
        user_id: Uuid,
        product_id: &str,
    ) -> Result<Option<Subscription>, DbError> {
        self.opt(
            "SELECT * FROM subscription WHERE user_id = $user_id \
               AND product_id = $product_id \
             ORDER BY purchase_time DESC, original_transaction_id ASC LIMIT 1",
            params!(user_id = user_id.to_string(), product_id = product_id,),
        )
        .await
    }
    pub async fn list_all_subscriptions(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Subscription>, DbError> {
        self.all(
            "SELECT * FROM subscription ORDER BY purchase_time DESC, \
                original_transaction_id ASC LIMIT $limit START $offset",
            params!(limit = limit, offset = offset),
        )
        .await
    }
    pub async fn update_user_wallet(
        &self,
        wallet: serde_json::Value,
        id: Uuid,
    ) -> Result<serde_json::Value, DbError> {
        let row: Users = self
            .one(
                "UPDATE type::record('users', <uuid>$id) SET wallet = $wallet, \
                    update_time = $update_time RETURN AFTER",
                params!(
                    id = id.to_string(),
                    wallet = wallet,
                    update_time = ts(Utc::now()),
                ),
            )
            .await?;
        Ok(row.wallet)
    }
}

impl Queries {
    /// Applies a wallet changeset and writes its ledger entry in one
    /// transaction. `deltas` is what the caller asked for, `base` the balance
    /// each delta was computed against and `result` the balance to store. Both
    /// statements carry the same guard, so either both write or neither does:
    /// an empty result means an unknown account, a currency that would go
    /// negative, or a balance that moved since it was read.
    pub async fn apply_wallet_change(
        &self,
        user_id: Uuid,
        deltas: &serde_json::Map<String, serde_json::Value>,
        base: &serde_json::Map<String, serde_json::Value>,
        result: &serde_json::Map<String, serde_json::Value>,
        metadata: serde_json::Value,
        ledger_id: Uuid,
    ) -> Result<Option<Users>, DbError> {
        let now = Utc::now();
        let game_id = self.account_game_id(user_id).await?;
        const GUARD: &str = "array::all(object::keys($result), |$c| ((wallet[$c] ?? 0) = $base[$c]) AND $result[$c] >= 0)";
        let rows: Vec<Users> = self
            .db
            .query_transaction(
                &format!(
                    "BEGIN TRANSACTION; \
                     INSERT INTO wallet_ledger ( \
                         SELECT type::record('wallet_ledger', <uuid>$ledger) AS id, \
                                $user AS user_id, $changeset AS changeset, $metadata AS metadata, \
                                $now AS create_time, $now AS update_time, $game_id AS game_id \
                         FROM type::record('users', <uuid>$id) WHERE {GUARD}) RETURN NONE; \
                     UPDATE type::record('users', <uuid>$id) \
                         SET wallet = object::extend(wallet, $result), update_time = $update_time \
                         WHERE {GUARD} RETURN AFTER; \
                     COMMIT TRANSACTION;"
                ),
                &params!(
                    id = user_id.to_string(),
                    user = user_id,
                    ledger = ledger_id.to_string(),
                    changeset = serde_json::Value::Object(deltas.clone()),
                    metadata = metadata,
                    now = time::encode(now),
                    game_id = game_id,
                    base = serde_json::Value::Object(base.clone()),
                    result = serde_json::Value::Object(result.clone()),
                    update_time = ts(now),
                ),
            )
            .await?;
        Ok(rows.into_iter().next())
    }

    pub async fn insert_wallet_ledger(&self, p: &InsertWalletLedgerParams) -> Result<(), DbError> {
        let now = Utc::now();
        let row = WalletLedger {
            id: p.id,
            user_id: p.user_id,
            changeset: p.changeset.clone(),
            metadata: p.metadata.clone(),
            create_time: now,
            update_time: now,
            game_id: self.account_game_id(p.user_id).await?,
        };
        self.create("wallet_ledger", Key::Uuid(p.id), &row).await?;
        Ok(())
    }
}
