use std::sync::OnceLock;

/// Phase-2 live-ops knobs. engine-config is read once at boot and is owned by
/// another lane's in-flight work, so these resolve from `OX_*` environment
/// variables with clamped defaults instead of a new config section.
pub(crate) struct LiveOps {
    pub idempotency_ttl_days: i64,
    /// Largest body the idempotency middleware will hash and replay. Anything
    /// bigger passes through, which keeps uploads out of the request buffer.
    pub idempotency_max_body_bytes: usize,
    pub market_ttl_days: i64,
    pub subscription_grace_days: i64,
    pub subscription_revalidation_days: i64,
    pub webhook_max_attempts: i64,
    pub webhook_backoff_ms: i64,
    pub webhook_timeout_ms: u64,
    pub webhook_concurrency: usize,
    pub webhook_poll_ms: u64,
    /// Outbound HTTP stability knobs, shared by every vendor call.
    pub http_breaker_trips: i64,
    pub http_breaker_cooldown_ms: u64,
    pub http_breaker_max_cooldown_ms: u64,
    pub http_bulkhead: usize,
    pub webhook_circuit_trips: i64,
    pub webhook_circuit_cooldown_ms: u64,
    pub notify_max_attempts: i64,
    pub notify_backoff_ms: i64,
    pub notify_timeout_ms: u64,
    pub notify_concurrency: usize,
    pub notify_poll_ms: u64,
    pub notify_unsubscribe_days: i64,
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
        idempotency_ttl_days: env_i64("CYG_IDEMPOTENCY_TTL_DAYS", 30, 1, 3650),
        idempotency_max_body_bytes: env_usize(
            "CYG_IDEMPOTENCY_MAX_BODY_BYTES",
            1_048_576,
            1_024,
            67_108_864,
        ),
        market_ttl_days: env_i64("CYG_MARKET_TTL_DAYS", 7, 1, 90),
        subscription_grace_days: env_i64("CYG_SUBSCRIPTION_GRACE_DAYS", 1, 0, 30),
        subscription_revalidation_days: env_i64("CYG_SUBSCRIPTION_REVALIDATION_DAYS", 7, 1, 365),
        webhook_max_attempts: env_i64("CYG_WEBHOOK_MAX_ATTEMPTS", 6, 1, 24),
        webhook_backoff_ms: env_i64("CYG_WEBHOOK_BACKOFF_MS", 1000, 100, 3_600_000),
        webhook_timeout_ms: env_u64("CYG_WEBHOOK_TIMEOUT_MS", 5000, 250, 60_000),
        webhook_concurrency: env_usize("CYG_WEBHOOK_CONCURRENCY", 8, 1, 64),
        webhook_poll_ms: env_u64("CYG_WEBHOOK_POLL_MS", 500, 50, 60_000),
        http_breaker_trips: env_i64("CYG_HTTP_BREAKER_TRIPS", 5, 1, 100),
        http_breaker_cooldown_ms: env_u64("CYG_HTTP_BREAKER_COOLDOWN_MS", 30_000, 100, 3_600_000),
        http_breaker_max_cooldown_ms: env_u64(
            "CYG_HTTP_BREAKER_MAX_COOLDOWN_MS",
            300_000,
            1_000,
            7_200_000,
        ),
        http_bulkhead: env_usize("CYG_HTTP_BULKHEAD", 32, 1, 1_024),
        webhook_circuit_trips: env_i64("CYG_WEBHOOK_CIRCUIT_TRIPS", 3, 1, 24),
        webhook_circuit_cooldown_ms: env_u64(
            "CYG_WEBHOOK_CIRCUIT_COOLDOWN_MS",
            30_000,
            100,
            3_600_000,
        ),
        notify_max_attempts: env_i64("CYG_NOTIFY_MAX_ATTEMPTS", 5, 1, 24),
        notify_backoff_ms: env_i64("CYG_NOTIFY_BACKOFF_MS", 2_000, 100, 3_600_000),
        notify_timeout_ms: env_u64("CYG_NOTIFY_TIMEOUT_MS", 10_000, 250, 120_000),
        notify_concurrency: env_usize("CYG_NOTIFY_CONCURRENCY", 4, 1, 32),
        notify_poll_ms: env_u64("CYG_NOTIFY_POLL_MS", 1_000, 50, 60_000),
        notify_unsubscribe_days: env_i64("CYG_NOTIFY_UNSUBSCRIBE_DAYS", 365, 1, 3_650),
    })
}
