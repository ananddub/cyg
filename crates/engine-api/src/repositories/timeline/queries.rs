use super::*;

/// Windowed reads for the operator player timeline. Each source returns at
/// most `limit` rows strictly older than `before` (when set), newest first —
/// the service merges the sources into one ordered feed and derives the next
/// cursor from the last entry it keeps.
impl Queries {
    pub async fn timeline_account_exists(&self, user_id: Uuid) -> Result<bool, DbError> {
        let rows: Vec<ExistsRow> = self
            .all(
                "SELECT id FROM type::record('users', <uuid>$user) LIMIT 1",
                params!(user = user_id.to_string()),
            )
            .await?;
        Ok(!rows.is_empty())
    }

    pub async fn timeline_ledger(
        &self,
        user_id: Uuid,
        before: Option<DateTime<Utc>>,
        limit: i64,
        game_id: Option<Uuid>,
    ) -> Result<Vec<WalletLedger>, DbError> {
        let mut surql = "SELECT * FROM wallet_ledger WHERE user_id = $user".to_owned();
        if game_id.is_some() {
            surql.push_str(" AND game_id = $game");
        }
        surql.push_str(&window_and_order("create_time", before));
        self.all(&surql, window_params(user_id, before, limit, game_id))
            .await
    }

    pub async fn timeline_inventory(
        &self,
        user_id: Uuid,
        before: Option<DateTime<Utc>>,
        limit: i64,
    ) -> Result<Vec<PlayerInventoryItem>, DbError> {
        self.all(
            &format!(
                "SELECT * FROM player_inventory_item WHERE user_id = $user{}",
                window_and_order("created_at", before)
            ),
            window_params(user_id, before, limit, None),
        )
        .await
    }

    pub async fn timeline_purchases(
        &self,
        user_id: Uuid,
        before: Option<DateTime<Utc>>,
        limit: i64,
    ) -> Result<Vec<Purchase>, DbError> {
        self.all(
            &format!(
                "SELECT * FROM purchase WHERE user_id = $user{}",
                window_and_order("create_time", before)
            ),
            window_params(user_id, before, limit, None),
        )
        .await
    }

    pub async fn timeline_subscriptions(
        &self,
        user_id: Uuid,
        before: Option<DateTime<Utc>>,
        limit: i64,
    ) -> Result<Vec<Subscription>, DbError> {
        self.all(
            &format!(
                "SELECT * FROM subscription WHERE user_id = $user{}",
                window_and_order("create_time", before)
            ),
            window_params(user_id, before, limit, None),
        )
        .await
    }

    pub async fn timeline_notifications(
        &self,
        user_id: Uuid,
        before: Option<DateTime<Utc>>,
        limit: i64,
    ) -> Result<Vec<Notification>, DbError> {
        self.all(
            &format!(
                "SELECT * FROM notification WHERE user_id = $user{}",
                window_and_order("create_time", before)
            ),
            window_params(user_id, before, limit, None),
        )
        .await
    }

    pub async fn timeline_messages(
        &self,
        user_id: Uuid,
        before: Option<DateTime<Utc>>,
        limit: i64,
    ) -> Result<Vec<Message>, DbError> {
        self.all(
            &format!(
                "SELECT * FROM message WHERE sender_id = $user{}",
                window_and_order("create_time", before)
            ),
            window_params(user_id, before, limit, None),
        )
        .await
    }
}

fn window_and_order(column: &str, before: Option<DateTime<Utc>>) -> String {
    let mut clause = String::new();
    if before.is_some() {
        clause.push_str(&format!(" AND {column} < $before"));
    }
    clause.push_str(&format!(" ORDER BY {column} DESC LIMIT $limit"));
    clause
}

#[derive(serde::Deserialize)]
struct ExistsRow {
    #[allow(dead_code)]
    id: Uuid,
}

fn window_params(
    user_id: Uuid,
    before: Option<DateTime<Utc>>,
    limit: i64,
    game_id: Option<Uuid>,
) -> std::collections::BTreeMap<String, Value> {
    let mut map = params!(user = user_id, limit = limit);
    if let Some(before) = before {
        map.insert("before".to_owned(), ts(before));
    }
    if let Some(game_id) = game_id {
        map.insert("game".to_owned(), serde_json::json!(game_id));
    }
    map
}
