use axum::Json;
use chrono::{DateTime, Utc};

use crate::ApiState;
use crate::dto::console::economy::{
    EconomySummary, EconomySummaryQuery, MarketActivity, PurchaseVolume, QuestActivity, WalletFlow,
};
use crate::error::V2Error;
use crate::repositories::economy::environment_name;
use crate::services::deps::q;

/// The ledger scan is bounded; the summary says so instead of pretending the
/// window was fully summed.
const ECONOMY_LEDGER_ROWS: i64 = 5000;

impl super::ConsoleService {
    pub async fn economy_summary(
        &self,
        state: &ApiState,
        query: EconomySummaryQuery,
    ) -> Result<Json<EconomySummary>, V2Error> {
        let to = match query.to.as_deref() {
            Some(raw) => parse_date_or_now(raw, true)?,
            None => Utc::now(),
        };
        let from = match query.from.as_deref() {
            Some(raw) => parse_date_or_now(raw, false)?,
            None => to - chrono::Duration::days(30),
        };
        if from > to {
            return Err(V2Error::invalid_argument(
                "The summary window starts after it ends.",
            ));
        }

        let queries = q(state).await?;
        let purchases: Vec<(String, i64)> = queries
            .count_purchases_by_environment(from, to)
            .await?
            .into_iter()
            .filter_map(|row| {
                row.environment
                    .map(|code| (environment_name(code).to_owned(), row.n))
            })
            .collect();
        let purchase_total: i64 = purchases.iter().map(|(_, n)| n).sum();
        let by_environment: std::collections::BTreeMap<String, i64> =
            purchases.into_iter().collect();

        let mut wallet = WalletFlow::default();
        let ledger = queries.ledger_window(from, to, ECONOMY_LEDGER_ROWS).await?;
        let mut net_by_currency: std::collections::BTreeMap<String, f64> = Default::default();
        for row in &ledger {
            let Some(changes) = row.changeset.as_object() else {
                continue;
            };
            for (currency, delta) in changes {
                let Some(delta) = delta.as_f64() else {
                    continue;
                };
                if delta >= 0.0 {
                    wallet.credits += 1;
                } else {
                    wallet.debits += 1;
                }
                *net_by_currency.entry(currency.clone()).or_default() += delta;
            }
        }
        wallet.net_by_currency = net_by_currency;

        let mut quests = QuestActivity::default();
        for row in queries.count_quest_states(from, to).await? {
            match row.state.as_deref() {
                Some("completed") => quests.completed += row.n,
                Some("claimed") => quests.claimed += row.n,
                _ => {}
            }
        }

        let mut market = MarketActivity::default();
        for row in queries.count_market_states(from, to).await? {
            match row.state.as_deref() {
                Some("sold") => market.trades_settled += row.n,
                _ => {}
            }
        }
        market.listings_created = queries
            .count_listings_created(from, to)
            .await?
            .first()
            .map(|row| row.n)
            .unwrap_or(0);
        for row in queries.count_wager_states(from, to).await? {
            match row.state.as_deref() {
                Some("held") => market.wagers_held += row.n,
                Some("settled") => market.wagers_settled += row.n,
                Some("refunded") => market.wagers_refunded += row.n,
                _ => {}
            }
        }

        let empty = purchase_total == 0
            && wallet.credits + wallet.debits == 0
            && quests.completed + quests.claimed == 0
            && market.listings_created
                + market.trades_settled
                + market.wagers_held
                + market.wagers_settled
                + market.wagers_refunded
                == 0;

        Ok(Json(EconomySummary {
            from,
            to,
            window_days: (to - from).num_days().max(0),
            purchases: PurchaseVolume {
                total: purchase_total,
                by_environment,
            },
            wallet,
            quests,
            market,
            empty,
        }))
    }
}

/// `from`/`to` accept a calendar date (`2026-10-01`) or a full RFC 3339
/// stamp; a date is read as the start (or end) of that UTC day.
fn parse_date_or_now(raw: &str, end_of_day: bool) -> Result<DateTime<Utc>, V2Error> {
    let raw = raw.trim();
    if let Ok(stamp) = DateTime::parse_from_rfc3339(raw) {
        return Ok(stamp.with_timezone(&Utc));
    }
    let date = chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d").map_err(|_| {
        V2Error::invalid_argument("A window bound is `YYYY-MM-DD` or an RFC 3339 timestamp.")
    })?;
    let midnight = date.and_hms_opt(0, 0, 0).expect("midnight is a valid time");
    let value = DateTime::<Utc>::from_naive_utc_and_offset(midnight, Utc);
    Ok(if end_of_day {
        value + chrono::Duration::days(1) - chrono::Duration::milliseconds(1)
    } else {
        value
    })
}
