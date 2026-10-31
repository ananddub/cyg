use std::time::Duration;

use crate::ApiState;

pub async fn gc_seats(state: ApiState) {
    let mut ticker = tokio::time::interval(Duration::from_secs(60));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        if let Ok(mut registry) = state.seats.lock() {
            registry.retire_expired();
        }
    }
}
