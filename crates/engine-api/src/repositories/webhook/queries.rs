use super::*;

/// The states a delivery moves through. Only `queued` is eligible to be sent.
pub const QUEUED: &str = "queued";
pub const SENDING: &str = "sending";
pub const DELIVERED: &str = "delivered";
pub const DEAD: &str = "dead";

impl Queries {
    /// Registers an endpoint, reporting `false` when its id is already taken.
    pub async fn create_webhook_endpoint(&self, row: &WebhookEndpoint) -> Result<bool, DbError> {
        match self
            .create("webhook_endpoint", Key::Uuid(row.id), row)
            .await
        {
            Ok(_) => Ok(true),
            Err(err) if err.is_unique() => Ok(false),
            Err(err) => Err(err),
        }
    }

    pub async fn webhook_endpoint(&self, id: Uuid) -> Result<Option<WebhookEndpoint>, DbError> {
        self.opt(
            "SELECT * FROM type::record('webhook_endpoint', <uuid>$id) LIMIT 1",
            params!(id = id.to_string()),
        )
        .await
    }

    pub async fn list_webhook_endpoints(&self) -> Result<Vec<WebhookEndpoint>, DbError> {
        self.all(
            "SELECT * FROM webhook_endpoint ORDER BY create_time ASC, id ASC",
            params!(),
        )
        .await
    }

    /// Writes the whole row back. `id` comes out of the content: the record key
    /// is stated by the target, and a plain string in its place would not be the
    /// same id at all.
    pub async fn update_webhook_endpoint(&self, row: &WebhookEndpoint) -> Result<(), DbError> {
        let mut data = serde_json::to_value(row)?;
        if let Value::Object(map) = &mut data {
            map.remove("id");
        }
        self.exec(
            "UPSERT type::record('webhook_endpoint', <uuid>$id) CONTENT $data",
            params!(id = row.id.to_string(), data = data),
        )
        .await
    }

    pub async fn delete_webhook_endpoint(&self, id: Uuid) -> Result<WriteResult, DbError> {
        affected(
            self,
            "DELETE type::record('webhook_endpoint', <uuid>$id) RETURN BEFORE",
            params!(id = id.to_string()),
        )
        .await
    }

    /// Queues one fact for one endpoint. `false` means the fact was already
    /// queued for it, which is what a replayed request looks like from here.
    pub async fn queue_delivery(&self, row: &WebhookDelivery) -> Result<bool, DbError> {
        match self
            .create("webhook_delivery", Key::Uuid(row.id), row)
            .await
        {
            Ok(_) => Ok(true),
            Err(err) if err.is_unique() => Ok(false),
            Err(err) => Err(err),
        }
    }

    /// Every endpoint that is switched on, oldest first. Which of them asked for
    /// a given fact is the caller's business: the list is a handful of rows and
    /// the matching is one string compare each.
    pub async fn active_endpoints(&self) -> Result<Vec<WebhookEndpoint>, DbError> {
        self.all(
            "SELECT * FROM webhook_endpoint WHERE is_active = true \
             ORDER BY create_time ASC, id ASC",
            params!(),
        )
        .await
    }

    /// Rows whose retry time has arrived. `sending` rows older than the reaper
    /// cutoff are owed by nobody any more: the process that claimed them did not
    /// come back, and the fact would otherwise sit claimed forever.
    pub async fn due_deliveries(&self, limit: i64) -> Result<Vec<WebhookDelivery>, DbError> {
        self.all(
            "SELECT * FROM webhook_delivery WHERE status = 'queued' AND next_retry_at <= $now \
             ORDER BY next_retry_at ASC, id ASC LIMIT $limit",
            params!(now = ts(Utc::now()), limit = limit),
        )
        .await
    }

    pub async fn requeue_stalled(&self, stalled_before: DateTime<Utc>) -> Result<(), DbError> {
        self.exec(
            "UPDATE webhook_delivery SET status = 'queued', update_time = $now \
             WHERE status = 'sending' AND update_time < $before",
            params!(now = ts(Utc::now()), before = ts(stalled_before)),
        )
        .await
    }

    /// Pushes an endpoint's owed facts past `until`, so a circuit that has just
    /// opened cannot have the same rows handed back to it on the next beat.
    pub async fn defer_endpoint(
        &self,
        endpoint_id: Uuid,
        until: DateTime<Utc>,
    ) -> Result<(), DbError> {
        self.exec(
            "UPDATE webhook_delivery SET next_retry_at = $until, update_time = $now \
             WHERE endpoint_id = $endpoint AND status = 'queued' AND next_retry_at <= $now",
            params!(
                endpoint = endpoint_id.to_string(),
                until = ts(until),
                now = ts(Utc::now()),
            ),
        )
        .await
    }

    /// Takes a queued row for this worker alone; a second worker reading the same
    /// due list gets nothing.
    pub async fn claim_delivery(&self, id: Uuid) -> Result<Option<WebhookDelivery>, DbError> {
        self.opt(
            "UPDATE type::record('webhook_delivery', <uuid>$id) \
             SET status = 'sending', update_time = $now WHERE status = 'queued' RETURN AFTER",
            params!(id = id.to_string(), now = ts(Utc::now())),
        )
        .await
    }

    /// Writes the outcome of one attempt. Guarded on the row still being this
    /// worker's, so a reaper that handed it to somebody else is not overwritten.
    pub async fn settle_delivery(
        &self,
        row: &WebhookDelivery,
        status: &str,
        attempt: i64,
        response_code: i64,
        last_error: &str,
        next_retry_at: DateTime<Utc>,
    ) -> Result<(), DbError> {
        self.exec(
            "UPDATE type::record('webhook_delivery', <uuid>$id) \
             SET status = $status, attempt = $attempt, response_code = $code, \
                 last_error = $error, next_retry_at = $next, update_time = $now \
             WHERE status = 'sending'",
            params!(
                id = row.id.to_string(),
                status = status,
                attempt = attempt,
                code = response_code,
                error = last_error,
                next = ts(next_retry_at),
                now = ts(Utc::now()),
            ),
        )
        .await
    }

    /// Writes a queued fact off the queue without ever sending it, which is what
    /// an endpoint the operator has since unregistered leaves behind.
    pub async fn abandon_delivery(&self, id: Uuid, why: &str) -> Result<(), DbError> {
        self.exec(
            "UPDATE type::record('webhook_delivery', <uuid>$id) \
             SET status = 'dead', attempt = 1, response_code = 0, last_error = $error, \
                 next_retry_at = $now, update_time = $now \
             WHERE status = 'queued'",
            params!(id = id.to_string(), error = why, now = ts(Utc::now())),
        )
        .await
    }

    /// Hands a finished row back to the queue when an operator asks for the fact
    /// to be sent again.
    pub async fn reopen_delivery(&self, id: Uuid) -> Result<bool, DbError> {
        let row: Option<WebhookDelivery> = self
            .opt(
                "UPDATE type::record('webhook_delivery', <uuid>$id) \
                 SET status = 'queued', next_retry_at = $now, update_time = $now \
                 WHERE status != 'sending' RETURN AFTER",
                params!(now = ts(Utc::now()), id = id.to_string()),
            )
            .await?;
        Ok(row.is_some())
    }

    pub async fn list_deliveries(
        &self,
        endpoint_id: Uuid,
        limit: i64,
    ) -> Result<Vec<WebhookDelivery>, DbError> {
        self.all(
            "SELECT * FROM webhook_delivery WHERE endpoint_id = $endpoint \
             ORDER BY create_time DESC, id ASC LIMIT $limit",
            params!(endpoint = endpoint_id.to_string(), limit = limit),
        )
        .await
    }

    pub async fn delivery(&self, id: Uuid) -> Result<Option<WebhookDelivery>, DbError> {
        self.opt(
            "SELECT * FROM type::record('webhook_delivery', <uuid>$id) LIMIT 1",
            params!(id = id.to_string()),
        )
        .await
    }
}
