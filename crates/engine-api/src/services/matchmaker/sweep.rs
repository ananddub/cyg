//! The background sweep task that drives matching every 5 seconds.

use super::TICKER_ONCE;

/// Start the sweep task exactly once per process.
pub(super) fn spawn_tick() {
    TICKER_ONCE.get_or_init(|| {
        tokio::spawn(async {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                super::try_match();
            }
        });
    });
}
