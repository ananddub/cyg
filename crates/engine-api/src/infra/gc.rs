use std::sync::Arc;
use std::time::Duration;

use crate::ApiState;

pub async fn gc_seats(state: ApiState) {
    let mut ticker = tokio::time::interval(Duration::from_secs(60));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        let mut cur = state.seats.load();
        loop {
            let mut next = (**cur).clone();
            next.retire_expired();
            let prev = state.seats.compare_and_swap(&*cur, Arc::new(next));
            if Arc::ptr_eq(&*cur, &*prev) {
                break;
            }
            cur = prev;
        }
    }
}
