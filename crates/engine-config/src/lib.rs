//! ox-game project configuration: `config.toml` (or flat `key: value`) into
//! typed structs.

mod console;
mod control;
mod game;
mod gameplay;
mod iap;
mod lifecycle;
mod maps;
mod module;
mod netcode;
/// The node's own name, for the room registry's owner column.
pub mod node;
mod parse;
mod rotation;
mod secret_audit;
mod session;
mod social;
mod templates;
mod watchdog;

pub use console::ConsoleConfig;
pub use control::{AgonesConfig, ControlConfig};
pub use game::{DEV_SEAT_SECRET, GameConfig, Runtime};
pub use gameplay::GameplayConfig;
pub use iap::IapConfig;
pub use lifecycle::LifecycleConfig;
pub use maps::{BootMaps, MapAsset, MapFormat};
pub use module::BootModule;
pub use netcode::{AoiConfig, RollbackConfig};
pub use node::node_id;
pub use parse::{load_config, parse_config};
pub use rotation::{CURRENT_KEY_ID, PREVIOUS_KEY_ID, Rotation};
pub use secret_audit::{Check, audit_secrets, placeholder_secret, weak_secret};
pub use session::SessionConfig;
pub use social::SocialConfig;
pub use templates::{CONFIG_TEMPLATE, ROOM_TEMPLATE, SCHEMA_TS_TEMPLATE};
pub use watchdog::WatchdogConfig;
