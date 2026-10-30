use crate::LiveIdentity;
use ahash::RandomState;
use serde::Deserialize;
use std::{collections::HashSet, sync::OnceLock};

pub(crate) type FastMap<K, V> = scc::HashMap<K, V, RandomState>;

pub(crate) static ACTIVE_LIVE_ROOMS: OnceLock<FastMap<String, ActiveLiveRoom>> = OnceLock::new();

#[inline]
pub(crate) fn active_live_rooms() -> &'static FastMap<String, ActiveLiveRoom> {
    ACTIVE_LIVE_ROOMS.get_or_init(|| FastMap::with_hasher(RandomState::new()))
}

#[derive(Debug, Deserialize)]
pub(crate) struct LiveSubscriptionRequest {
    pub(crate) endpoint: String,
    pub(crate) args: serde_json::Value,
}
#[derive(Clone, Debug)]
pub(crate) struct ActiveLiveRoom {
    pub(crate) namespace: &'static str,
    pub(crate) room: String,
    pub(crate) endpoint: String,
    pub(crate) args: serde_json::Value,
    pub(crate) identity: Option<LiveIdentity>,
    subscribers: usize,
}
#[derive(Clone, Debug, Default)]
pub(crate) struct SocketSubscriptions(pub(crate) HashSet<String>);

pub(crate) fn retain(
    namespace: &'static str,
    room: &str,
    request: &LiveSubscriptionRequest,
    identity: Option<LiveIdentity>,
) {
    let rooms = active_live_rooms();
    match rooms.entry_sync(room.to_owned()) {
        scc::hash_map::Entry::Occupied(mut occ) => {
            occ.get_mut().subscribers += 1;
        }
        scc::hash_map::Entry::Vacant(vac) => {
            vac.insert_entry(ActiveLiveRoom {
                namespace,
                room: room.to_owned(),
                endpoint: request.endpoint.clone(),
                args: request.args.clone(),
                identity,
                subscribers: 1,
            });
        }
    }
}
pub(crate) fn release(room: &str) {
    let rooms = active_live_rooms();
    let mut should_remove = false;
    let _ = rooms.update_sync(room, |_, active| {
        if active.subscribers > 1 {
            active.subscribers -= 1;
        } else {
            should_remove = true;
        }
    });
    if should_remove {
        let _ = rooms.remove_sync(room);
    }
}
pub(crate) fn scoped(
    identity: Option<&LiveIdentity>,
    endpoint: &str,
    args: &serde_json::Value,
) -> String {
    match identity {
        Some(identity) => format!("user:{}:{endpoint}:{args}", identity.user_id),
        None => format!("{endpoint}:{args}"),
    }
}
