/// `[aoi]` block: per-viewer delta culling instead of one whole-room buffer.
#[derive(Debug, Clone, Copy)]
pub struct AoiConfig {
    /// Off keeps the legacy whole-room broadcast.
    pub enabled: bool,
    /// Sent every tick inside this radius.
    pub near_radius: f64,
    /// Between `near_radius` and this, sent every `skip_ticks`-th tick.
    pub far_radius: f64,
    pub skip_ticks: u64,
    /// Projectile cull radius around each viewer.
    pub bullet_radius: f64,
}

/// `[rollback]` block: GGPO-style late-input rewind on the authoritative host.
#[derive(Debug, Clone, Copy)]
pub struct RollbackConfig {
    /// Off keeps the legacy newest-wins drop for stale seqs.
    pub enabled: bool,
    /// Inputs older than this are dropped; replay cost grows linearly.
    pub max_replay_ticks: u64,
    pub journal_size: usize,
}

impl Default for AoiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            near_radius: 180.0,
            far_radius: 420.0,
            skip_ticks: 4,
            bullet_radius: 260.0,
        }
    }
}

impl Default for RollbackConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_replay_ticks: 32,
            journal_size: 256,
        }
    }
}
