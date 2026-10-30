use super::*;

impl Queries {
    pub async fn load_listing(&self, listing_id: Uuid) -> Result<Option<MarketListing>, DbError> {
        self.opt(
            "SELECT * FROM type::record('market_listing', <uuid>$id) LIMIT 1",
            params!(id = listing_id.to_string()),
        )
        .await
    }

    pub async fn create_listing(&self, row: &MarketListing) -> Result<(), DbError> {
        self.create("market_listing", Key::Uuid(row.id), row)
            .await?;
        Ok(())
    }

    /// Takes a seller's instance into escrow: the flip is guarded on the row
    /// still belonging to the seller, so a borrowed or already-moved instance
    /// yields nothing and no listing can be opened against it.
    pub async fn hold_item(
        &self,
        instance_id: Uuid,
        seller: Uuid,
        escrow: Uuid,
    ) -> Result<Option<PlayerInventoryItem>, DbError> {
        self.opt(
            "UPDATE type::record('player_inventory_item', <uuid>$instance) \
             SET user_id = $escrow, updated_at = $now \
             WHERE user_id = $seller AND quantity >= 1 RETURN AFTER",
            params!(
                instance = instance_id.to_string(),
                escrow = escrow.to_string(),
                seller = seller.to_string(),
                now = ts(Utc::now()),
            ),
        )
        .await
    }

    /// Hands a held instance back to `recipient`. Guarded on the current holder
    /// so the item cannot jump to two owners.
    pub async fn move_held_item(
        &self,
        instance_id: Uuid,
        from: Uuid,
        to: Uuid,
    ) -> Result<bool, DbError> {
        let held: Option<PlayerInventoryItem> = self
            .opt(
                "UPDATE type::record('player_inventory_item', <uuid>$instance) \
                 SET user_id = $to, updated_at = $now \
                 WHERE user_id = $from RETURN AFTER",
                params!(
                    instance = instance_id.to_string(),
                    to = to.to_string(),
                    from = from.to_string(),
                    now = ts(Utc::now()),
                ),
            )
            .await?;
        Ok(held.is_some())
    }

    /// Closes an open listing exactly once; the item move that follows is
    /// compensated by the caller if it fails.
    pub async fn flip_listing(
        &self,
        listing_id: Uuid,
        to: &str,
    ) -> Result<Option<MarketListing>, DbError> {
        self.opt(
            "UPDATE type::record('market_listing', <uuid>$id) \
             SET state = $to, update_time = $now WHERE state = 'open' RETURN AFTER",
            params!(id = listing_id.to_string(), to = to, now = ts(Utc::now()),),
        )
        .await
    }

    /// Opens a closed listing again when the item move failed underneath it.
    pub async fn reopen_listing(&self, listing_id: Uuid) -> Result<(), DbError> {
        self.exec(
            "UPDATE type::record('market_listing', <uuid>$id) \
             SET state = 'open', update_time = $now WHERE state != 'open'",
            params!(id = listing_id.to_string(), now = ts(Utc::now())),
        )
        .await
    }

    pub async fn create_wager(&self, row: &Wager) -> Result<(), DbError> {
        self.create("wager", Key::Uuid(row.id), row).await?;
        Ok(())
    }

    pub async fn load_wager(&self, wager_id: Uuid) -> Result<Option<Wager>, DbError> {
        self.opt(
            "SELECT * FROM type::record('wager', <uuid>$id) LIMIT 1",
            params!(id = wager_id.to_string()),
        )
        .await
    }

    /// Takes a held wager to its closing state exactly once.
    pub async fn flip_wager(
        &self,
        wager_id: Uuid,
        to: &str,
        outcome: &serde_json::Value,
    ) -> Result<Option<Wager>, DbError> {
        self.opt(
            "UPDATE type::record('wager', <uuid>$id) SET state = $to, outcome = $outcome, \
                 update_time = $now WHERE state = 'held' RETURN AFTER",
            params!(
                id = wager_id.to_string(),
                to = to,
                outcome = outcome.clone(),
                now = ts(Utc::now()),
            ),
        )
        .await
    }

    /// Opens a settled or refunded wager back up when its payout failed, so a
    /// retried close is possible instead of a dead end.
    pub async fn reopen_wager(&self, wager_id: Uuid) -> Result<(), DbError> {
        self.exec(
            "UPDATE type::record('wager', <uuid>$id) SET state = 'held', update_time = $now \
             WHERE state != 'held'",
            params!(id = wager_id.to_string(), now = ts(Utc::now())),
        )
        .await
    }
}
