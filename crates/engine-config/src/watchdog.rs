
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct WatchdogConfig {

    pub enabled: bool,

    pub tick_budget_ms: u64,

    pub epoch_deadline_ticks: u64,

    pub max_ticks_per_frame: u32,
}

impl Default for WatchdogConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            tick_budget_ms: 500,
            epoch_deadline_ticks: 4,
            max_ticks_per_frame: 3,
        }
    }
}

impl WatchdogConfig {

    pub fn clamp(&mut self) {
        self.tick_budget_ms = self.tick_budget_ms.clamp(250, 60_000);
        self.epoch_deadline_ticks = self.epoch_deadline_ticks.clamp(1, 64);
        self.max_ticks_per_frame = self.max_ticks_per_frame.clamp(1, 16);
    }

    pub fn epoch_period(&self) -> Duration {
        let micros = (self.tick_budget_ms.saturating_mul(1_000)) / self.epoch_deadline_ticks.max(1);
        Duration::from_micros(micros.max(1_000))
    }

    pub fn apply_env(&mut self) {
        if let Ok(v) = std::env::var("OX_WATCHDOG_ENABLED")
            && (v == "0" || v == "false")
        {
            self.enabled = false;
        }
        if let Ok(v) = std::env::var("OX_WATCHDOG_BUDGET_MS")
            && let Ok(ms) = v.parse()
        {
            self.tick_budget_ms = ms;
        }
        if let Ok(v) = std::env::var("OX_WATCHDOG_TICKS")
            && let Ok(ticks) = v.parse()
        {
            self.epoch_deadline_ticks = ticks;
        }
        if let Ok(v) = std::env::var("OX_MAX_TICKS_PER_FRAME")
            && let Ok(cap) = v.parse()
        {
            self.max_ticks_per_frame = cap;
        }
    }

    pub fn from_env() -> WatchdogConfig {
        let mut watchdog = WatchdogConfig::default();
        watchdog.apply_env();
        watchdog.clamp();
        watchdog
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_watchdog_is_on_by_default_with_a_budget_no_honest_tick_reaches() {
        let watchdog = WatchdogConfig::default();
        assert!(watchdog.enabled);
        assert!(
            watchdog.tick_budget_ms > 10 * (1_000 / 30),
            "a 30 Hz tick must be far from the fuse"
        );
    }

    #[test]
    fn the_slices_add_up_to_the_budget() {
        let watchdog = WatchdogConfig {
            tick_budget_ms: 500,
            epoch_deadline_ticks: 4,
            ..WatchdogConfig::default()
        };
        assert_eq!(watchdog.epoch_period(), Duration::from_millis(125));
        assert_eq!(
            watchdog.epoch_period() * watchdog.epoch_deadline_ticks as u32,
            Duration::from_millis(watchdog.tick_budget_ms)
        );
    }

    #[test]
    fn an_absurd_budget_is_pulled_back_instead_of_trusting_the_operator() {
        let mut watchdog = WatchdogConfig {
            enabled: true,
            tick_budget_ms: 1,
            epoch_deadline_ticks: 0,
            max_ticks_per_frame: 0,
        };
        watchdog.clamp();
        assert_eq!(watchdog.tick_budget_ms, 250);
        assert_eq!(watchdog.epoch_deadline_ticks, 1);
        assert_eq!(watchdog.max_ticks_per_frame, 1);
        assert_eq!(watchdog.epoch_period(), Duration::from_millis(250));

        let mut huge = WatchdogConfig {
            tick_budget_ms: u64::MAX,
            max_ticks_per_frame: u32::MAX,
            ..WatchdogConfig::default()
        };
        huge.clamp();
        assert_eq!(huge.tick_budget_ms, 60_000);
        assert_eq!(huge.max_ticks_per_frame, 16);
    }
}
