use super::{RoomHostData, TimerState};

impl RoomHostData {
    pub fn timer_set(&mut self, timer_id: i64, delay_ticks: i64, interval_ticks: i64) {
        let delay = delay_ticks.max(1);
        self.timers.insert(
            timer_id,
            TimerState {
                next_due: self.clock_ticks + delay,
                interval: interval_ticks.max(0),
            },
        );
        self.fired_timers.retain(|id| *id != timer_id);
    }

    pub fn timer_cancel(&mut self, timer_id: i64) {
        self.timers.remove(&timer_id);
        self.fired_timers.retain(|id| *id != timer_id);
    }

    pub fn timer_poll(&mut self) -> i64 {
        if self.fired_timers.is_empty() {
            let due: Vec<i64> = self
                .timers
                .iter()
                .filter(|(_, t)| self.clock_ticks >= t.next_due)
                .map(|(id, _)| *id)
                .collect();
            for id in due {
                if let Some(t) = self.timers.get_mut(&id) {
                    if t.interval > 0 {
                        t.next_due = self.clock_ticks + t.interval;
                    } else {
                        self.timers.remove(&id);
                    }
                }
                self.fired_timers.push(id);
            }
        }
        if self.fired_timers.is_empty() {
            0
        } else {
            self.fired_timers.remove(0)
        }
    }
}
