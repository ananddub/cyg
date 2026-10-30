use std::collections::BTreeMap;

/// The scopes the API charges a player for. A route names one of these when it
/// spends part of the player's budget, so an operator tunes each surface
/// separately.
pub const LOGIN: &str = "login";
pub const CHANNEL_SEND: &str = "channel.send";
pub const STORAGE_WRITE: &str = "storage.write";
pub const WEBHOOK_RETRY: &str = "webhook.retry";

/// One window: at most `cap` charges may be spent per `period_secs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuotaScope {
    pub cap: u32,
    pub period_secs: u64,
}

/// `[control.quotas]`: per-player budgets for the expensive routes. Off by
/// default, because a quota that fires on a game's busiest route is a decision
/// the operator makes, not one the engine makes for them.
#[derive(Debug, Clone)]
pub struct QuotaConfig {
    pub enabled: bool,
    /// Operator sessions are bucketed like a player's only when this is off.
    pub exempt_console: bool,
    /// Requests per second and burst allowed to one authenticated account.
    pub player_rps: u32,
    pub player_burst: u32,
    pub scopes: BTreeMap<String, QuotaScope>,
    /// Applied to a scope nobody listed here, so a new charged route still has
    /// a window the moment it is wired.
    pub fallback: QuotaScope,
}

impl QuotaConfig {
    pub fn window(&self, scope: &str) -> QuotaScope {
        self.scopes.get(scope).copied().unwrap_or(self.fallback)
    }
}

impl Default for QuotaConfig {
    fn default() -> Self {
        let scopes = [
            (
                LOGIN,
                QuotaScope {
                    cap: 20,
                    period_secs: 300,
                },
            ),
            (
                CHANNEL_SEND,
                QuotaScope {
                    cap: 60,
                    period_secs: 60,
                },
            ),
            (
                STORAGE_WRITE,
                QuotaScope {
                    cap: 50,
                    period_secs: 300,
                },
            ),
            (
                WEBHOOK_RETRY,
                QuotaScope {
                    cap: 30,
                    period_secs: 60,
                },
            ),
        ]
        .into_iter()
        .map(|(name, scope)| (name.to_string(), scope))
        .collect();
        Self {
            enabled: false,
            exempt_console: true,
            player_rps: 20,
            player_burst: 40,
            scopes,
            fallback: QuotaScope {
                cap: 60,
                period_secs: 60,
            },
        }
    }
}
