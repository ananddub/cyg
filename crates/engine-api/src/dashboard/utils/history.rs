use std::sync::{Arc, OnceLock};

use engine_dashboard::StatusSnapshot;

static HISTORY: OnceLock<Arc<engine_dashboard::DashboardHistory>> = OnceLock::new();

pub fn history() -> Arc<engine_dashboard::DashboardHistory> {
    HISTORY.get_or_init(Default::default).clone()
}

pub fn record_sample(snapshot: StatusSnapshot) {
    // Same monotonic clock as `sample_age_ms`, so a chart point and the age
    // reported next to it cannot disagree.
    history().push(snapshot, engine_bus::clock::mono_ns() / 1_000_000);
}
