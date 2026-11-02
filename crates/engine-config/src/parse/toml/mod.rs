mod blocks;
mod control;
mod scalars;

use crate::game::GameConfig;

pub(crate) fn fill(cfg: &mut GameConfig, table: &toml::value::Table) {
    scalars::apply(cfg, table);
    blocks::apply(cfg, table);
    control::apply(cfg, table);
}
