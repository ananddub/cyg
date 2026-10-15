//! The two shared sender registries the hot downlink path reads.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;

use super::inbox::SessionInbound;

/// Registries the hot downlink path reads without going through the demux. These
/// hold *senders*, never session state — locking them does not serialize two
/// sessions against each other.
#[derive(Clone)]
pub(crate) struct Registries {
    /// session id -> its inbox sender.
    pub(crate) outbox: Arc<Mutex<HashMap<Arc<str>, mpsc::Sender<SessionInbound>>>>,
    /// player id -> session id.
    pub(crate) player_index: Arc<Mutex<HashMap<Arc<str>, Arc<str>>>>,
}

impl Registries {
    pub(crate) fn lock_outbox(
        &self,
    ) -> std::sync::MutexGuard<'_, HashMap<Arc<str>, mpsc::Sender<SessionInbound>>> {
        self.outbox.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(crate) fn lock_players(&self) -> std::sync::MutexGuard<'_, HashMap<Arc<str>, Arc<str>>> {
        self.player_index.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Remove a session's player bindings once its task is gone.
pub(crate) fn unbind_player(session_id: &Arc<str>, registries: &Registries) {
    let mut players = registries.lock_players();
    players.retain(|_, sid| sid != session_id);
    registries.lock_outbox().remove(session_id);
}
