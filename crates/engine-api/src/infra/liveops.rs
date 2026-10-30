use std::sync::OnceLock;

/// Phase-2 live-ops knobs. engine-config is read once at boot and is owned by
/// another lane's in-flight work, so these resolve from `OX_*` environment
/// variables with clamped defaults instead of a new config section.
pub(crate) struct LiveOps {
    pub idempotency_ttl_days: i64,
}

fn env_i64(name: &str, default: i64, min: i64, max: i64) -> i64 {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.trim().parse::<i64>().ok())
        .map(|value| value.clamp(min, max))
        .unwrap_or(default)
}

pub(crate) fn config() -> &'static LiveOps {
    static CONFIG: OnceLock<LiveOps> = OnceLock::new();
    CONFIG.get_or_init(|| LiveOps {
        idempotency_ttl_days: env_i64("OX_IDEMPOTENCY_TTL_DAYS", 30, 1, 3650),
    })
}
