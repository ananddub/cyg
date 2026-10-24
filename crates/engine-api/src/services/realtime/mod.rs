//! Realtime WebSocket event orchestration services.
//!
//! Dispatches inbound `auto_ws` messages to their owning service planes (chat, matches, party, etc.).

pub mod chat;
pub mod matches;
pub mod matchmaker;
pub mod notifications;
pub mod party;
pub mod presence;
pub mod sessions;

pub use chat::RealtimeChatService;
pub use matches::RealtimeMatchService;
pub use matchmaker::RealtimeMatchmakerService;
pub use notifications::RealtimeNotificationService;
pub use party::RealtimePartyService;
pub use presence::RealtimePresenceService;
pub use sessions::RealtimeSessionHub;

use ahash::RandomState;
use scc::{HashMap, HashSet, TreeIndex};

/// Hardware AES-accelerated concurrent hash map.
pub type FastMap<K, V> = HashMap<K, V, RandomState>;

pub fn new_fast_map<K: Eq + std::hash::Hash, V>() -> FastMap<K, V> {
    FastMap::with_hasher(RandomState::new())
}

/// Hardware AES-accelerated concurrent hash set.
pub type FastSet<T> = HashSet<T, RandomState>;

pub fn new_fast_set<T: Eq + std::hash::Hash>() -> FastSet<T> {
    HashSet::with_hasher(RandomState::new())
}

pub type FastTree<K, V> = TreeIndex<K, V>;

pub fn new_fast_tree<K: Ord + Clone, V>() -> FastTree<K, V> {
    TreeIndex::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use scc::Guard;

    #[test]
    fn test_tree_index() {
        let tree: FastTree<i32, String> = new_fast_tree();
        let _ = tree.insert_sync(10, "ten".into());
        let _ = tree.insert_sync(20, "twenty".into());
        let _ = tree.insert_sync(30, "thirty".into());
        assert!(tree.read_sync(&10, |_, v| v.clone()).is_some());
        let guard = Guard::new();
        let count = tree.range(15..=35, &guard).count();
        assert_eq!(count, 2);
        let _ = tree.remove_sync(&10);
    }
}
