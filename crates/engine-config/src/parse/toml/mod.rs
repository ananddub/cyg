mod blocks;
mod control;
mod features;
mod scalars;

use crate::game::GameConfig;

mod rooms;

pub(crate) fn fill(cfg: &mut GameConfig, table: &toml::value::Table) {
    scalars::apply(cfg, table);
    blocks::apply(cfg, table);
    control::apply(cfg, table);
    features::apply(cfg, table);
    rooms::apply(cfg, table);
}
