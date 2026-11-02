use crate::game::GameConfig;

pub(super) fn apply(cfg: &mut GameConfig, key: &str, value: &str) {
    match key {
        "max_ticks_per_frame" => {
            cfg.watchdog.max_ticks_per_frame =
                value.parse().unwrap_or(cfg.watchdog.max_ticks_per_frame)
        }
        "watchdog_enabled" => cfg.watchdog.enabled = value.parse().unwrap_or(cfg.watchdog.enabled),
        "tick_budget_ms" => {
            cfg.watchdog.tick_budget_ms = value.parse().unwrap_or(cfg.watchdog.tick_budget_ms)
        }
        "epoch_deadline_ticks" => {
            cfg.watchdog.epoch_deadline_ticks =
                value.parse().unwrap_or(cfg.watchdog.epoch_deadline_ticks)
        }
        _ => {}
    }
}
