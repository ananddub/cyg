use super::*;

pub enum ClaimOutcome {
    Acquired,
    Replay(IdempotencyKey),
}

impl Queries {
    /// Takes the claim for this key. A racing second CREATE surfaces either as
    /// a unique violation or as a store write conflict, so any failure re-reads
    /// the row rather than reporting an internal error.
    pub async fn claim_idempotency(&self, row: &IdempotencyKey) -> Result<ClaimOutcome, DbError> {
        match self
            .create("idempotency_key", Key::String(row.id.clone()), row)
            .await
        {
            Ok(_) => Ok(ClaimOutcome::Acquired),
            Err(err) => match self.get_idempotency(&row.id).await? {
                Some(stored) => Ok(ClaimOutcome::Replay(stored)),
                None => Err(err),
            },
        }
    }

    pub async fn get_idempotency(&self, id: &str) -> Result<Option<IdempotencyKey>, DbError> {
        self.opt(
            "SELECT * FROM type::record('idempotency_key', $id) LIMIT 1",
            params!(id = id),
        )
        .await
    }

    pub async fn complete_idempotency(&self, id: &str, response: &Value) -> Result<(), DbError> {
        self.exec(
            "UPDATE type::record('idempotency_key', $id) SET status = 'done', \
             response = $response, update_time = $now",
            params!(id = id, response = response.clone(), now = ts(Utc::now())),
        )
        .await
    }

    pub async fn release_idempotency(&self, id: &str) -> Result<(), DbError> {
        self.exec(
            "DELETE type::record('idempotency_key', $id) WHERE status = 'pending'",
            params!(id = id),
        )
        .await
    }

    pub async fn prune_idempotency(&self, older_than: DateTime<Utc>) -> Result<(), DbError> {
        self.exec(
            "DELETE idempotency_key WHERE create_time < $cutoff",
            params!(cutoff = ts(older_than)),
        )
        .await
    }
}
