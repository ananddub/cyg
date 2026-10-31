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
            params!(
                user_id = user_id.to_string(),
                product_id = product_id,
            ),
        )
        .await
    }
    pub async fn list_all_subscriptions(
        &self,
        limit: i64,        offset: i64,
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
