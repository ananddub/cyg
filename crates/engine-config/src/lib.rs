//! ox-game project configuration: `config.toml` (or flat `key: value`) into
//! typed structs.

mod control;
mod game;
mod netcode;
mod parse;
mod templates;

pub use control::{AgonesConfig, ControlConfig};
pub use game::{DEV_SEAT_SECRET, GameConfig, Runtime};
pub use netcode::{AoiConfig, RollbackConfig};
pub use parse::{load_config, parse_config};
pub use templates::{CONFIG_TEMPLATE, ROOM_TEMPLATE};
