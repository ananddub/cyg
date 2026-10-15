//! Client presence data on a joined stream.

use std::sync::Arc;

use serde_json::Value;

use crate::error::V2Error;
use crate::infra::ws::envelope::{Envelope, EnvelopeMessage};
use crate::infra::ws::session;

use super::helpers::stream_of;
use super::state::broadcast;

/// Relay `stream_data` to the other members of the matching channel, no
/// persistence.
pub fn stream_data(
    s: &Arc<session::Session>,
    stream_id: &str,
    data: Value,
    reliable: bool,
    cid: Option<String>,
) -> Result<(), V2Error> {
    if stream_of(stream_id).is_none() {
        return Err(V2Error::not_found(
            "must join the stream before sending data",
        ));
    }
    broadcast(
        stream_id,
        &Envelope {
            cid: None,
            message: EnvelopeMessage::StreamData {
                stream_id: stream_id.to_string(),
                data: data.clone(),
                reliable,
            },
        },
        Some(s.sid),
    );
    // The sender only gets its own send confirmed when it asks reliably.
    if reliable {
        s.send_message(
            cid,
            EnvelopeMessage::StreamData {
                stream_id: stream_id.to_string(),
                data,
                reliable,
            },
        );
    }
    Ok(())
}
