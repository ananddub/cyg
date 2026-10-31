pub mod schema;
pub mod service;
pub mod tenant;

pub use schema::{ensure_schema, ensure_tenant_schema};
pub use service::GamesService;

pub const GAME_KEY_PREFIX: &str = "gm_";

pub const API_KEY_PREFIX: &str = "cyg_sec_";

pub const CLIENT_KEY_PREFIX: &str = "cyg_pub_";
