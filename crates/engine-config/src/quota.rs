use std::collections::BTreeMap;

pub const LOGIN: &str = "login";
pub const CHANNEL_SEND: &str = "channel.send";
pub const STORAGE_WRITE: &str = "storage.write";
pub const WEBHOOK_RETRY: &str = "webhook.retry";

pub const NOTIFY_SEND: &str = "notify.send";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuotaScope {
    pub cap: u32,
    pub period_secs: u64,
}

#[derive(Debug, Clone)]
pub struct QuotaConfig {
    pub enabled: bool,

    pub exempt_console: bool,

    pub player_rps: u32,
    pub player_burst: u32,
    pub scopes: BTreeMap<String, QuotaScope>,

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
            (
                NOTIFY_SEND,
                QuotaScope {
                    cap: 120,
                    period_secs: 3600,
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
