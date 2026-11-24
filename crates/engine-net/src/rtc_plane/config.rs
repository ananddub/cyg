#[derive(Debug, Clone, Copy)]
pub struct PlaneConfig {
    pub session_inbox_capacity: usize,
}

impl Default for PlaneConfig {
    fn default() -> Self {
        Self {
            session_inbox_capacity: 512,
        }
    }
}
