//! The games a console operator registers — the tenancy root of the game plane.
//!
//! A game is what everything else hangs off of: a room module belongs to one, a seat is
//! issued by one, and the secret a client authenticates with is a game's. This module
//! owns the table bootstrap ([`schema`]) and the console's CRUD ([`service`]); the
//! module-upload surface extends the same [`GamesService`] rather than growing a second
//! path to the same rows.

pub mod schema;
pub mod service;
pub mod tenant;

pub use schema::{ensure_schema, ensure_tenant_schema};
pub use service::GamesService;

/// The prefix of a game's tenant key. Naming it here is what lets the console's
/// display code and the module plane agree on what a key looks like without either
/// reading the other's SQL.
pub const GAME_KEY_PREFIX: &str = "gm_";

/// The prefix of a game's secret — enough of a shape to recognise a credential in a
/// log line, without the credential being in the line.
pub const API_KEY_PREFIX: &str = "cyg_sec_";

/// The prefix of a game's public client credential. Unlike the secret it ships inside
/// the built game and rides on every console listing; it identifies the tenant to the
/// relay plane rather than authenticating it.
pub const CLIENT_KEY_PREFIX: &str = "cyg_pub_";
