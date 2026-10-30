use ahash::RandomState;
use auto_di::BoxFuture;
use socketioxide::SocketIo;
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, OnceLock},
};

pub(crate) type FastMap<K, V> = scc::HashMap<K, V, RandomState>;

pub(crate) static SOCKET_IO: OnceLock<SocketIo> = OnceLock::new();
pub(crate) static LIVE_REFRESHERS: OnceLock<Vec<ResolvedLiveRefresher>> = OnceLock::new();
pub(crate) static LATEST_CHANNELS: OnceLock<
    FastMap<String, tokio::sync::watch::Sender<serde_json::Value>>,
> = OnceLock::new();
pub(crate) static STREAM_CHANNELS: OnceLock<
    FastMap<String, tokio::sync::mpsc::Sender<serde_json::Value>>,
> = OnceLock::new();
pub(crate) static STREAM_HISTORY: OnceLock<FastMap<String, StreamHistory>> = OnceLock::new();
pub(crate) static ACCESS_ENDPOINTS: OnceLock<
    HashMap<&'static str, Option<(&'static str, &'static str)>>,
> = OnceLock::new();
pub(crate) static SUBSCRIPTION_REFRESH_GATES: OnceLock<
    FastMap<String, Arc<tokio::sync::Mutex<()>>>,
> = OnceLock::new();
pub(crate) static CACHE_INVALIDATOR: OnceLock<CacheInvalidator> = OnceLock::new();
pub(crate) static LIVE_ENDPOINT_CACHE: OnceLock<moka::future::Cache<String, serde_json::Value>> =
    OnceLock::new();

#[inline]
pub(crate) fn latest_channels()
-> &'static FastMap<String, tokio::sync::watch::Sender<serde_json::Value>> {
    LATEST_CHANNELS.get_or_init(|| FastMap::with_hasher(RandomState::new()))
}

#[inline]
pub(crate) fn stream_channels()
-> &'static FastMap<String, tokio::sync::mpsc::Sender<serde_json::Value>> {
    STREAM_CHANNELS.get_or_init(|| FastMap::with_hasher(RandomState::new()))
}

#[inline]
pub(crate) fn stream_history() -> &'static FastMap<String, StreamHistory> {
    STREAM_HISTORY.get_or_init(|| FastMap::with_hasher(RandomState::new()))
}

#[inline]
pub(crate) fn subscription_refresh_gates() -> &'static FastMap<String, Arc<tokio::sync::Mutex<()>>>
{
    SUBSCRIPTION_REFRESH_GATES.get_or_init(|| FastMap::with_hasher(RandomState::new()))
}

pub(crate) struct StreamHistory {
    pub(crate) limit: usize,
    pub(crate) events: VecDeque<serde_json::Value>,
}
pub(crate) type LiveRefresher = Arc<
    dyn Fn(serde_json::Value, Option<crate::LiveIdentity>) -> BoxFuture<'static, ()>
        + Send
        + Sync
        + 'static,
>;
pub(crate) type CacheInvalidator =
    Arc<dyn Fn(Vec<String>) -> BoxFuture<'static, ()> + Send + Sync + 'static>;
pub(crate) struct ResolvedLiveRefresher {
    pub(crate) endpoint: &'static str,
    pub(crate) refresh: LiveRefresher,
}
