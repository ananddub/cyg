//! Socket teardown: leave every channel the session stood in.

use std::sync::Arc;

use crate::infra::ws::envelope::{Envelope, EnvelopeMessage};
use crate::infra::ws::session;

use super::state::{broadcast, remove_member};

pub fn cleanup(s: &Arc<session::Session>) {
    let joined: Vec<String> = s
        .channels
        .read()
        .expect("channel lock")
        .iter()
        .cloned()
        .collect();
    for channel_id in joined {
        if let Some(p) = remove_member(&channel_id, s.sid) {
            broadcast(
                &channel_id,
                &Envelope {
                    cid: None,
                    message: EnvelopeMessage::ChannelPresenceEvent {
                        channel_id: channel_id.clone(),
                        joins: Vec::new(),
                        leaves: vec![p],
                    },
                },
                None,
            );
        }
        s.channels
            .write()
            .expect("channel lock")
            .remove(&channel_id);
    }
}
