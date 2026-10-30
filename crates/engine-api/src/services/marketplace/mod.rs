use std::sync::Arc;

use auto_di::singleton;
use serde_json::json;
use uuid::Uuid;

use crate::db::DbError;
use crate::error::V2Error;
use crate::repositories::db::Queries;
use crate::services::quest::reward::payout_uuid;

pub mod listings;
pub mod sale;
pub mod wagers;

pub struct MarketplaceService;

#[singleton]
impl MarketplaceService {
    fn new() -> Self {
        Self
    }
}

/// The balance triple `apply_wallet_change` guards itself with: what to write,
/// what must still be there, and what may never fall below zero.
pub(crate) async fn transfer(
    queries: &Arc<Queries>,
    user: Uuid,
    currency: &str,
    delta: f64,
    seed: &str,
) -> Result<(), V2Error> {
    let account = match queries.get_user_by_id(user).await {
        Ok(row) => row,
        Err(DbError::NotFound) => return Err(V2Error::not_found("Account not found.")),
        Err(err) => return Err(err.into()),
    };
    let held = account
        .wallet
        .as_object()
        .and_then(|wallet| wallet.get(currency))
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0);
    let mut changes = serde_json::Map::new();
    let mut base = serde_json::Map::new();
    let mut result = serde_json::Map::new();
    changes.insert(currency.to_string(), json!(delta));
    base.insert(currency.to_string(), json!(held));
    result.insert(currency.to_string(), json!(held + delta));
    let metadata = json!({ "reason": seed });
    let applied = queries
        .apply_wallet_change(
            user,
            &changes,
            &base,
            &result,
            metadata,
            payout_uuid(&format!("{seed}/ledger")),
        )
        .await?;
    if applied.is_some() {
        return Ok(());
    }
    if delta < 0.0 {
        return Err(V2Error::failed_precondition(
            "A currency balance cannot go negative.",
        ));
    }
    Err(V2Error::unavailable(
        "The balance moved while it was being paid; retry.",
    ))
}
