//! ox-game project configuration: `config.toml` (or flat `key: value`) into
//! typed structs.

mod console;
mod control;
mod game;
mod iap;
mod maps;
mod module;
mod netcode;
mod parse;
mod session;
mod social;
mod templates;

pub use console::ConsoleConfig;
pub use control::{AgonesConfig, ControlConfig};
pub use game::{DEV_SEAT_SECRET, GameConfig, Runtime};
pub use iap::IapConfig;
pub use maps::{BootMaps, MapAsset, MapFormat};
pub use module::BootModule;
pub use netcode::{AoiConfig, RollbackConfig};
pub use parse::{load_config, parse_config};
pub use session::SessionConfig;
pub use social::SocialConfig;
pub use templates::{CONFIG_TEMPLATE, ROOM_TEMPLATE, SCHEMA_TS_TEMPLATE};
