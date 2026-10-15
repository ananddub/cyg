//! `/ws` — the realtime event table, declared with `auto_ws`.
//!
//! `auto_ws` decodes and routes each frame by `type`, so an `#[on]` method only
//! reads its payload and calls the owning plane; `#[connect]`/`#[disconnect]`
//! register and tear down the session.

use std::sync::Arc;

use auto_ws::Conn;

use crate::ApiState;
use crate::error::V2Error;
use crate::infra::ws::envelope::EnvelopeMessage;
use crate::infra::ws::session;

pub mod handlers;
pub mod messages;
pub mod socket;

/// `Ok(Some(m))` replies `m` on the cid; `Ok(None)` means the plane already
/// answered; `Err(e)` is an error reply on the same cid.
pub(crate) type Dispatched = Result<Option<EnvelopeMessage>, V2Error>;

/// The matchmaker answers failures as a ready envelope, not a `V2Error`.
pub(crate) fn unwrap_env_err(msg: EnvelopeMessage) -> V2Error {
    match msg {
        EnvelopeMessage::Error(e) => V2Error::internal(e.message),
        other => V2Error::internal(format!("unexpected matchmaker reply: {other:?}")),
    }
}

/// The session registered for this connection at `connect`.
pub(crate) fn session_of(conn: &Conn) -> Arc<session::Session> {
    conn.get::<Arc<session::Session>>()
        .expect("session is registered in #[connect]")
}

/// The plane state the authenticator stashed at upgrade time.
pub(crate) fn state_of(conn: &Conn) -> ApiState {
    conn.get::<ApiState>()
        .expect("plane state is stashed by the authenticator")
}
