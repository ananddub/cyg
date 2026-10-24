//! The fixtures the authentication tests share: the socket plane and the default
//! credentials, plus the body-slicing helper the GameCenter test builds requests with.

pub(super) use super::super::server::V2Server;
pub(super) use super::super::{player_token, server_key, v2_state};
pub(super) use super::first_fields;
