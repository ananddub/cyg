//! Session tokens, wire-compatible with the: an HS256 JWT carrying
//! `{tid, uid, usn, vrs, exp, iat}` — the claim names, order and omission rules
//! of the `SessionTokenClaims`. The two tokens of a pair share a `tid` and
//! an `iat`; [`crate::infra::session_cache`] blacklists the shared `tid` to revoke.

mod claims;
mod password;
mod token;
mod verify;

pub use claims::Claims;
pub use password::{hash_password, verify_password};
pub use token::{configure, mint_token_pair, mint_token_pair_with};
pub use verify::{parse_refresh, parse_session, verify_refresh, verify_session};
