use auto_di::BoxFuture;
use socketioxide::SocketIo;
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex, OnceLock},
};

pub(crate) static SOCKET_IO: OnceLock<SocketIo> = OnceLock::new();
pub(crate) static LIVE_REFRESHERS: OnceLock<Vec<ResolvedLiveRefresher>> = OnceLock::new();
pub(crate) static LATEST_CHANNELS: OnceLock<
    Mutex<HashMap<String, tokio::sync::watch::Sender<serde_json::Value>>>,
> = OnceLock::new();
pub(crate) static STREAM_CHANNELS: OnceLock<
    Mutex<HashMap<String, tokio::sync::mpsc::Sender<serde_json::Value>>>,
> = OnceLock::new();
pub(crate) static STREAM_HISTORY: OnceLock<Mutex<HashMap<String, StreamHistory>>> = OnceLock::new();
pub(crate) static ACCESS_ENDPOINTS: OnceLock<
    HashMap<&'static str, Option<(&'static str, &'static str)>>,
> = OnceLock::new();
pub(crate) static SUBSCRIPTION_REFRESH_GATES: OnceLock<
    Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
> = OnceLock::new();
pub(crate) static CACHE_INVALIDATOR: OnceLock<CacheInvalidator> = OnceLock::new();
pub(crate) static LIVE_ENDPOINT_CACHE: OnceLock<moka::future::Cache<String, serde_json::Value>> =
    OnceLock::new();

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
