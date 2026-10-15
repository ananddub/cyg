//! Shared socket-channel registry and fan-out helpers.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

use uuid::Uuid;

use crate::infra::ws::envelope::{Envelope, Presence};
use crate::infra::ws::session;
use crate::utils::channel::Stream;

pub(super) struct ChannelState {
    pub(super) stream: Stream,
    pub(super) members: HashMap<Uuid /*sid*/, Presence>,
}

static CHANNELS: OnceLock<RwLock<HashMap<String, ChannelState>>> = OnceLock::new();

pub(super) fn channels() -> &'static RwLock<HashMap<String, ChannelState>> {
    CHANNELS.get_or_init(|| RwLock::new(HashMap::new()))
}

pub(super) fn remove_member(channel_id: &str, sid: Uuid) -> Option<Presence> {
    let mut all = channels().write().expect("channel registry lock");
    let left = match all.get_mut(channel_id) {
        Some(c) => c.members.remove(&sid),
        None => return None,
    };
    if all.get(channel_id).is_some_and(|c| c.members.is_empty()) {
        all.remove(channel_id);
    }
    left
}

/// Send one envelope to every member of the channel except `skip`
/// (None = everyone).
pub(super) fn broadcast(channel_id: &str, env: &Envelope, skip: Option<Uuid>) {
    let all = channels().read().expect("channel registry lock");
    let Some(c) = all.get(channel_id) else { return };
    for sid in c.members.keys() {
        if Some(*sid) != skip {
            if let Some(sess) = session::find(*sid) {
                sess.send(env);
            }
        }
    }
}
