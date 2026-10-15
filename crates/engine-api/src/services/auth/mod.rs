//! The ten `POST /v2/account/authenticate*` endpoints: decode the provider body,
//! prove the server key, then find-or-create the account and mint a token pair.

pub mod local;
mod queries;
mod service;
pub mod social;
pub mod validate;

pub use queries::q;
pub use service::AuthService;
