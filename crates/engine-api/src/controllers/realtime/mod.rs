use std::sync::Arc;

use auto_ws::Conn;

use crate::ApiState;
use crate::error::V2Error;
use crate::infra::ws::envelope::EnvelopeMessage;
use crate::infra::ws::session;

pub mod messages;
pub mod socket;

pub(crate) type Dispatched = Result<Option<EnvelopeMessage>, V2Error>;

pub(crate) fn unwrap_env_err(msg: EnvelopeMessage) -> V2Error {
    match msg {
        EnvelopeMessage::Error(e) => V2Error::internal(e.message),
        other => V2Error::internal(format!("unexpected matchmaker reply: {other:?}")),
    }
}

pub(crate) fn session_of(conn: &Conn) -> Arc<session::Session> {
    conn.get::<Arc<session::Session>>()
        .expect("session is registered in #[connect]")
}

pub(crate) fn state_of(conn: &Conn) -> ApiState {
    conn.get::<ApiState>()
        .expect("plane state is stashed by the authenticator")
}
