//! `[watchdog]`: the fuse on a guest that never returns.
//!
//! A room's logic runs in-process, so a guest stuck in a loop stops the host
//! loop and every other room on it. Wasmtime's epoch interruption can cut such a
//! call off, but only if the deadline is armed and something advances the epoch;
//! before this block existed neither happened, so a bad `on_tick` hung the
//! process until the container was killed.

use std::time::Duration;

/// How hard the guest-timeout fuse is set.
#[derive(Debug, Clone)]
pub struct WatchdogConfig {
    /// Kill guest calls that outrun their budget. Off leaves the host exactly as
    /// it was before: a stuck guest hangs the process.
    pub enabled: bool,
    /// Longest a single guest call — `on_tick` above all — may run. This is a
    /// fuse, not a rate limit: the default is far above any honest tick
    /// (`on_tick` at 30 Hz needs ~33 ms) because killing a room mid-tick leaves
    /// torn state, and a machine that briefly stalls should not lose matches.
    pub tick_budget_ms: u64,
    /// How many slices the budget is cut into. The fuse can only fire on a slice
    /// boundary, so more slices mean a sharper deadline and more epoch churn.
    pub epoch_deadline_ticks: u64,
}

impl Default for WatchdogConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            tick_budget_ms: 500,
            epoch_deadline_ticks: 4,
        }
    }
}

impl WatchdogConfig {
    /// Keep the fuse inside the range the engine can act on. A budget below one
    /// host tick would kill healthy rooms, and a deadline of zero would trap at
    /// the first epoch check.
    pub fn clamp(&mut self) {
        self.tick_budget_ms = self.tick_budget_ms.clamp(250, 60_000);
        self.epoch_deadline_ticks = self.epoch_deadline_ticks.clamp(1, 64);
    }

    /// How often the epoch pump advances, so that `epoch_deadline_ticks` slices
    /// add up to `tick_budget_ms`.
    pub fn epoch_period(&self) -> Duration {
        let micros = (self.tick_budget_ms.saturating_mul(1_000)) / self.epoch_deadline_ticks.max(1);
        Duration::from_micros(micros.max(1_000))
    }

    /// Environment overrides, shared by the `config.toml` path and the env-only
    /// sim plane. Only the off-switch is read as a flag: any other value keeps
    /// the watchdog on, so a typo cannot silently disable it.
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
    }

    /// Read the block from the environment on top of the shipped defaults.
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
        };
        watchdog.clamp();
        assert_eq!(watchdog.tick_budget_ms, 250);
        assert_eq!(watchdog.epoch_deadline_ticks, 1);
        assert_eq!(watchdog.epoch_period(), Duration::from_millis(250));

        let mut huge = WatchdogConfig {
            tick_budget_ms: u64::MAX,
            ..WatchdogConfig::default()
        };
        huge.clamp();
        assert_eq!(huge.tick_budget_ms, 60_000);
    }
}
