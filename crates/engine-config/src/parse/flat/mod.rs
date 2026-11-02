mod basic;
mod control;
mod gameplay;
mod lifecycle;
mod watchdog;

use crate::game::GameConfig;

pub(crate) fn apply(cfg: &mut GameConfig, key: &str, value: &str) {
    basic::apply(cfg, key, value);
    lifecycle::apply(cfg, key, value);
    watchdog::apply(cfg, key, value);
    gameplay::apply(cfg, key, value);
    control::apply(cfg, key, value);
}
