use crate::LiveIdentity;
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    sync::{Mutex, OnceLock},
};

pub(crate) static ACTIVE_LIVE_ROOMS: OnceLock<Mutex<HashMap<String, ActiveLiveRoom>>> =
    OnceLock::new();

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
    let mut rooms = ACTIVE_LIVE_ROOMS
        .get_or_init(Default::default)
        .lock()
        .expect("active live room registry poisoned");
    rooms
        .entry(room.to_owned())
        .and_modify(|active| active.subscribers += 1)
        .or_insert_with(|| ActiveLiveRoom {
            namespace,
            room: room.to_owned(),
            endpoint: request.endpoint.clone(),
            args: request.args.clone(),
            identity,
            subscribers: 1,
        });
}
pub(crate) fn release(room: &str) {
    let mut rooms = ACTIVE_LIVE_ROOMS
        .get_or_init(Default::default)
        .lock()
        .expect("active live room registry poisoned");
    let remove = rooms.get_mut(room).is_some_and(|active| {
        if active.subscribers > 1 {
            active.subscribers -= 1;
            false
        } else {
            true
        }
    });
    if remove {
        rooms.remove(room);
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
