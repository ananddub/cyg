
#[derive(Debug, Clone, Copy)]
pub struct AoiConfig {

    pub enabled: bool,

    pub near_radius: f64,

    pub far_radius: f64,
    pub skip_ticks: u64,

    pub bullet_radius: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct RollbackConfig {

    pub enabled: bool,

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
