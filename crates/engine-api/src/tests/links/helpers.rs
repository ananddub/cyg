//! The fixtures the link tests share: the socket plane, the default credentials
//! and the refusal a guarded write gives with no DSN.

pub(super) use super::super::authenticate::first_fields;
pub(super) use super::super::server::V2Server;
pub(super) use super::super::{player_token, server_key, v2_state};

pub(super) const NO_DB: &str = "The database is not configured, set [control.session] database_url.";
