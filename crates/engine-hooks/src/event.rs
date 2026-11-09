//! The account events a hook can react to.
//!
//! These are deliberately *not* room events. A room's lifecycle already has
//! `onCreate` / `onTick` / `onInput` inside the game module; these are the things
//! that happen to a player while no particular room is involved — a purchase, a
//! level, a match result. Nakama calls this split "runtime hooks vs match
//! handlers" and PlayFab calls it "CloudScript vs the game server"; the split is
//! the same and it exists because neither side can do the other's job.

use serde::{Deserialize, Serialize};

/// Every event the server can raise into customer logic.
///
/// The set is closed on purpose: a hook fires at a known point in a known
/// transaction, which is what lets the server state honestly whether a hook's
/// answer was applied before or after the write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookEvent {
    /// A validated store purchase was recorded.
    Purchase,
    /// Virtual currency changed, whoever moved it.
    WalletChange,
    /// A player reached a new level.
    LevelUp,
    /// A match finished and its result is about to be recorded.
    MatchEnd,
    /// A leaderboard record is about to be written.
    LeaderboardSubmit,
    /// A player registered for the first time.
    PlayerCreate,
    /// A player was banned, muted or otherwise moderated.
    Moderation,
}

impl HookEvent {
    pub const ALL: [HookEvent; 7] = [
        HookEvent::Purchase,
        HookEvent::WalletChange,
        HookEvent::LevelUp,
        HookEvent::MatchEnd,
        HookEvent::LeaderboardSubmit,
        HookEvent::PlayerCreate,
        HookEvent::Moderation,
    ];

    /// The guest export name that answers this event, e.g. `on_purchase`.
    pub fn export(self) -> &'static str {
        match self {
            HookEvent::Purchase => "on_purchase",
            HookEvent::WalletChange => "on_wallet_change",
            HookEvent::LevelUp => "on_level_up",
            HookEvent::MatchEnd => "on_match_end",
            HookEvent::LeaderboardSubmit => "on_leaderboard_submit",
            HookEvent::PlayerCreate => "on_player_create",
            HookEvent::Moderation => "on_moderation",
        }
    }

    /// Stable name used in config, logs and the dashboard.
    pub fn as_str(self) -> &'static str {
        match self {
            HookEvent::Purchase => "purchase",
            HookEvent::WalletChange => "wallet_change",
            HookEvent::LevelUp => "level_up",
            HookEvent::MatchEnd => "match_end",
            HookEvent::LeaderboardSubmit => "leaderboard_submit",
            HookEvent::PlayerCreate => "player_create",
            HookEvent::Moderation => "moderation",
        }
    }

    pub fn parse(name: &str) -> Option<HookEvent> {
        let normalised = name.trim().to_ascii_lowercase().replace('-', "_");
        HookEvent::ALL
            .into_iter()
            .find(|event| event.as_str() == normalised)
    }
}

/// What the server hands the hook.
///
/// JSON in, JSON out: the hook runs in the same sandbox as a room module but has
/// no access to any room's memory, so the only channel is this document. A
/// hook cannot read the database directly — it answers, and the server decides
/// whether to act on the answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookInput {
    pub event: HookEvent,
    /// The player the event concerns.
    pub player_id: String,
    /// Event-specific fields. Kept free-form so a new field does not need a new
    /// ABI version; the guest reads what it knows.
    pub payload: serde_json::Value,
}

impl HookInput {
    pub fn new(event: HookEvent, player_id: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            event,
            player_id: player_id.into(),
            payload,
        }
    }
}

/// What a hook is allowed to do.
///
/// The guest never writes; it *proposes*. The host applies each action through
/// the same services the REST API uses, which means a hook cannot bypass a
/// quota, a validation or an audit entry.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct HookOutput {
    /// Currency movements, applied atomically with the event that raised them.
    pub grant_currency: Vec<CurrencyGrant>,
    /// Inventory changes.
    pub grant_items: Vec<ItemGrant>,
    /// Badges to attach.
    pub grant_badges: Vec<String>,
    /// Notifications to send. `player_id` empty means the event's player.
    pub notify: Vec<Notification>,
    /// Extra fields merged into the event's audit record.
    pub metadata: serde_json::Map<String, serde_json::Value>,
    /// Set to refuse the event. The server rolls the whole transaction back.
    pub reject: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrencyGrant {
    pub currency: String,
    pub amount: i64,
    /// Free-form reason, stored on the ledger row so a balance can be explained.
    #[serde(default)]
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemGrant {
    pub item_id: String,
    #[serde(default = "one")]
    pub quantity: i64,
}

fn one() -> i64 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    #[serde(default)]
    pub player_id: String,
    pub subject: String,
    #[serde(default)]
    pub body: String,
}
