#[derive(Debug, Clone)]
pub struct ControlConfig {
    pub enabled: bool,
    pub bind_addr: String,

    /// Explicit dev opt-in: allow placeholder secrets to boot even when the
    /// server is bound to a non-loopback address. Same power as the
    /// CYG_ALLOW_DEFAULT_SECRETS=1 environment variable, but lives in
    /// config.toml so `cyg run` works without exporting anything. Never set
    /// this on a public box — it is the boot guard's off switch.
    pub dev_mode: bool,

    pub api_port: u16,
    pub workers: usize,
    pub seat_ttl_secs: u64,

    pub seat_secret: String,
    pub rate_limit_rps: u32,
    pub rate_limit_burst: u32,
    pub agones: AgonesConfig,
    pub quotas: crate::quota::QuotaConfig,
    pub session: crate::session::SessionConfig,
    pub console: crate::console::ConsoleConfig,
    pub iap: crate::iap::IapConfig,
    pub social: crate::social::SocialConfig,
}

#[derive(Debug, Clone)]
pub struct AgonesConfig {
    pub enabled: bool,
    pub http_port: u16,

    pub health_interval_ms: u64,
    pub host: String,
}

impl Default for AgonesConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            http_port: 9358,
            health_interval_ms: 2000,
            host: "127.0.0.1".into(),
        }
    }
}

impl Default for ControlConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            // Loopback: the *default* must never be the exposed one. An operator
            // putting this on a public host opts in by editing config.toml (or
            // setting the env var), and then also has to supply real secrets.
            bind_addr: "127.0.0.1".into(),
            dev_mode: false,
            api_port: 8124,
            workers: 2,
            seat_ttl_secs: 300,
            seat_secret: crate::game::DEV_SEAT_SECRET.into(),
            rate_limit_rps: 20,
            rate_limit_burst: 40,
            agones: AgonesConfig::default(),
            quotas: crate::quota::QuotaConfig::default(),
            session: crate::session::SessionConfig::default(),
            console: crate::console::ConsoleConfig::default(),
            iap: crate::iap::IapConfig::default(),
            social: crate::social::SocialConfig::default(),
        }
    }
}
