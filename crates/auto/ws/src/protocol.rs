use axum::http::request::Parts;
use axum::response::Response;

use crate::{Identity, Msg, WsError};

/// The three things the runtime cannot know about a socket: how a request is
/// authenticated, how a frame is decoded, and how a failure is written back.
///
/// A protocol is a type, not a value — [`mount`](crate::mount) takes it as a
/// type parameter, so there is no global to configure and two sockets may
/// speak two different protocols in one binary.
pub trait Protocol: Send + Sync + 'static {
    /// Decide whether the upgrade request becomes a socket, and what
    /// [`Identity`] it carries. The error is served as the HTTP response in
    /// place of the upgrade, so authentication failures never open a socket.
    ///
    /// The plane state is readable here through `current_state()` when the
    /// socket is mounted into a state-attached router; stash anything the
    /// connection will need on the returned identity. The query string is
    /// `parts.uri.query()`.
    fn authenticate(parts: &Parts) -> Result<Identity, Response>;

    /// Decode one text frame. Returning `Err` answers with
    /// [`encode_error`](Self::encode_error) on no `cid`.
    fn decode(frame: &str) -> Result<Msg, WsError>;

    /// Encode a failure frame for `cid`.
    fn encode_error(cid: Option<&str>, error: &WsError) -> String;
}
