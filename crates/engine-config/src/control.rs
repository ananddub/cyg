/// `[control]` block: the Tokio/Axum browser-facing plane.
#[derive(Debug, Clone)]
pub struct ControlConfig {
    pub enabled: bool,
    pub bind_addr: String,
    /// Own port by default; set equal to `web_port` to hand the website over
    /// to Axum. `0` asks the kernel (tests).
    pub api_port: u16,
    pub workers: usize,
    pub seat_ttl_secs: u64,
    /// `OX_SEAT_SECRET` overrides this at load time.
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

/// `[control.agones]`: Kubernetes Dedicated Game Server sidecar client.
#[derive(Debug, Clone)]
pub struct AgonesConfig {
    pub enabled: bool,
    pub http_port: u16,
    /// Agones marks the pod unhealthy after three missed pings — keep well
    /// under 10 s.
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
            bind_addr: "0.0.0.0".into(),
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
