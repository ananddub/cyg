//! The two shared sender registries the hot downlink path reads.

use std::sync::Arc;

use ahash::RandomState;
use tokio::sync::mpsc;

use super::inbox::SessionInbound;

/// Fast concurrent map type for WebRTC registries.
pub(crate) type FastMap<K, V> = scc::HashMap<K, V, RandomState>;

/// Registries the hot downlink path reads without going through the demux.
/// Concurrently accessible and 100% lock-free via `scc::HashMap`.
#[derive(Clone)]
pub(crate) struct Registries {
    /// session id -> its inbox sender.
    pub(crate) outbox: Arc<FastMap<Arc<str>, mpsc::Sender<SessionInbound>>>,
    /// player id -> session id.
    pub(crate) player_index: Arc<FastMap<Arc<str>, Arc<str>>>,
}

impl Default for Registries {
    fn default() -> Self {
        Self {
            outbox: Arc::new(FastMap::with_hasher(RandomState::new())),
            player_index: Arc::new(FastMap::with_hasher(RandomState::new())),
        }
    }
}

impl Registries {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub(crate) fn insert_session(&self, session_id: Arc<str>, tx: mpsc::Sender<SessionInbound>) {
        let _ = self.outbox.insert_sync(session_id, tx);
    }

    #[inline]
    pub(crate) fn bind_player(&self, player_id: Arc<str>, session_id: Arc<str>) {
        let _ = self.player_index.insert_sync(player_id, session_id);
    }

    #[inline]
    pub(crate) fn player_to_session(&self, player_id: &str) -> Option<Arc<str>> {
        self.player_index.read_sync(player_id, |_, sid| sid.clone())
    }

    #[inline]
    pub(crate) fn route_inbox<R>(
        &self,
        session_id: &str,
        f: impl FnOnce(&mpsc::Sender<SessionInbound>) -> R,
    ) -> Option<R> {
        self.outbox.read_sync(session_id, |_, tx| f(tx))
    }

    #[inline]
    pub(crate) fn session_count(&self) -> usize {
        self.outbox.len()
    }
}

/// Remove a session's player bindings once its task is gone.
pub(crate) fn unbind_player(session_id: &Arc<str>, registries: &Registries) {
    registries
        .player_index
        .retain_sync(|_, sid| sid != session_id);
    let _ = registries.outbox.remove_sync(session_id);
}
