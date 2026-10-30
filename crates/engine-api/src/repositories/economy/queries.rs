use super::*;

/// Aggregates for `GET /v2/console/economy/summary`. Counts run as SurrealQL
/// `GROUP BY` queries; the wallet ledger deltas are summed in Rust because
/// they live inside a JSON changeset column.
impl Queries {
    pub async fn count_purchases_by_environment(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<CountRow>, DbError> {
        self.all(
            "SELECT count() AS n, environment FROM purchase \
             WHERE create_time >= $from AND create_time <= $to \
             GROUP BY environment",
            window(from, to),
        )
        .await
    }

    pub async fn count_quest_states(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<CountRow>, DbError> {
        self.all(
            "SELECT count() AS n, state FROM player_quest \
             WHERE update_time >= $from AND update_time <= $to \
             GROUP BY state",
            window(from, to),
        )
        .await
    }

    pub async fn count_market_states(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<CountRow>, DbError> {
        self.all(
            "SELECT count() AS n, state FROM market_listing \
             WHERE update_time >= $from AND update_time <= $to \
             GROUP BY state",
            window(from, to),
        )
        .await
    }

    pub async fn count_wager_states(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<CountRow>, DbError> {
        self.all(
            "SELECT count() AS n, state FROM wager \
             WHERE update_time >= $from AND update_time <= $to \
             GROUP BY state",
            window(from, to),
        )
        .await
    }

    pub async fn count_listings_created(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<CountRow>, DbError> {
        self.all(
            "SELECT count() AS n FROM market_listing \
             WHERE create_time >= $from AND create_time <= $to \
             GROUP ALL",
            window(from, to),
        )
        .await
    }

    /// The ledger rows of the window, oldest first. The caller bounds this at
    /// `ECONOMY_LEDGER_ROWS`; the summary reports when it had to stop early.
    pub async fn ledger_window(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<WalletLedger>, DbError> {
        self.all(
            "SELECT * FROM wallet_ledger \
             WHERE create_time >= $from AND create_time <= $to \
             ORDER BY create_time ASC LIMIT $limit",
            params!(from = ts(from), to = ts(to), limit = limit),
        )
        .await
    }
}

/// The environment names the IAP surface already uses: 1 sandbox, 2
/// production, anything else unknown.
pub fn environment_name(code: i64) -> &'static str {
    match code {
        1 => "SANDBOX",
        2 => "PRODUCTION",
        _ => "UNKNOWN",
    }
}

#[derive(serde::Deserialize)]
pub struct CountRow {
    #[serde(default)]
    pub n: i64,
    #[serde(default)]
    pub environment: Option<i64>,
    #[serde(default)]
    pub state: Option<String>,
}

fn window(from: DateTime<Utc>, to: DateTime<Utc>) -> std::collections::BTreeMap<String, Value> {
    params!(from = ts(from), to = ts(to))
}
