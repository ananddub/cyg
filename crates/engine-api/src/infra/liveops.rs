use std::sync::OnceLock;

/// Phase-2 live-ops knobs. engine-config is read once at boot and is owned by
/// another lane's in-flight work, so these resolve from `OX_*` environment
/// variables with clamped defaults instead of a new config section.
pub(crate) struct LiveOps {
    pub idempotency_ttl_days: i64,
    pub market_ttl_days: i64,
    pub subscription_grace_days: i64,
    pub subscription_revalidation_days: i64,
    pub webhook_max_attempts: i64,
    pub webhook_backoff_ms: i64,
    pub webhook_timeout_ms: u64,
    pub webhook_concurrency: usize,
    pub webhook_poll_ms: u64,
    pub webhook_circuit_trips: i64,
    pub webhook_circuit_cooldown_ms: u64,
}

fn env_i64(name: &str, default: i64, min: i64, max: i64) -> i64 {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.trim().parse::<i64>().ok())
        .map(|value| value.clamp(min, max))
        .unwrap_or(default)
}

fn env_u64(name: &str, default: u64, min: u64, max: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .map(|value| value.clamp(min, max))
        .unwrap_or(default)
}

fn env_usize(name: &str, default: usize, min: usize, max: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .map(|value| value.clamp(min, max))
        .unwrap_or(default)
}

pub(crate) fn config() -> &'static LiveOps {
    static CONFIG: OnceLock<LiveOps> = OnceLock::new();
    CONFIG.get_or_init(|| LiveOps {
        idempotency_ttl_days: env_i64("OX_IDEMPOTENCY_TTL_DAYS", 30, 1, 3650),
        market_ttl_days: env_i64("OX_MARKET_TTL_DAYS", 7, 1, 90),
        subscription_grace_days: env_i64("OX_SUBSCRIPTION_GRACE_DAYS", 1, 0, 30),
        subscription_revalidation_days: env_i64("OX_SUBSCRIPTION_REVALIDATION_DAYS", 7, 1, 365),
        webhook_max_attempts: env_i64("OX_WEBHOOK_MAX_ATTEMPTS", 6, 1, 24),
        webhook_backoff_ms: env_i64("OX_WEBHOOK_BACKOFF_MS", 1000, 100, 3_600_000),
        webhook_timeout_ms: env_u64("OX_WEBHOOK_TIMEOUT_MS", 5000, 250, 60_000),
        webhook_concurrency: env_usize("OX_WEBHOOK_CONCURRENCY", 8, 1, 64),
        webhook_poll_ms: env_u64("OX_WEBHOOK_POLL_MS", 500, 50, 60_000),
        webhook_circuit_trips: env_i64("OX_WEBHOOK_CIRCUIT_TRIPS", 3, 1, 24),
        webhook_circuit_cooldown_ms: env_u64(
            "OX_WEBHOOK_CIRCUIT_COOLDOWN_MS",
            30_000,
            100,
            3_600_000,
        ),
    })
}
