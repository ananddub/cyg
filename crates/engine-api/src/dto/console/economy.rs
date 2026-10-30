use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Default, Deserialize)]
pub struct EconomySummaryQuery {
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
    #[serde(default)]
    pub game_id: Option<uuid::Uuid>,
}

#[derive(Debug, Serialize)]
pub struct EconomySummary {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub window_days: i64,
    pub purchases: PurchaseVolume,
    pub wallet: WalletFlow,
    /// Desync reports in the window — the pressure side of the funnel.
    pub desyncs: i64,
    pub quests: QuestActivity,
    pub market: MarketActivity,
    /// `0` when the window holds no activity at all — the UI renders zeros,
    /// never errors, for an empty window.
    pub empty: bool,
}

#[derive(Debug, Serialize, Default)]
pub struct PurchaseVolume {
    pub total: i64,
    pub refunded: i64,
    pub by_environment: BTreeMap<String, i64>,
}

#[derive(Debug, Serialize, Default)]
pub struct WalletFlow {
    pub credits: i64,
    pub debits: i64,
    /// Net change per currency code, straight from the ledger changesets.
    pub net_by_currency: BTreeMap<String, f64>,
}

#[derive(Debug, Serialize, Default)]
pub struct QuestActivity {
    pub completed: i64,
    pub claimed: i64,
}

#[derive(Debug, Serialize, Default)]
pub struct MarketActivity {
    pub listings_created: i64,
    pub trades_settled: i64,
    pub wagers_held: i64,
    pub wagers_settled: i64,
    pub wagers_refunded: i64,
}
