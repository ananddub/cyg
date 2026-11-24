//! One SurrealDB LIVE query per `(topic, game)`, broadcast to every client.
//!
//! The feed socket subscribes with a plain filter: *which game*. Before this hub
//! every subscription opened its own LIVE query, so a thousand players watching a
//! game meant a thousand identical queries. Here the first watcher opens the query;
//! everyone after that joins the broadcast channel. The query is torn down a short
//! time after the last watcher leaves, so a game with no audience costs nothing.
//!
//! The LIVE query stays in place because it already carries the record. A poll on
//! a timer would ask the same question again and again and still miss the moment
//! between two ticks.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use scc::HashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::broadcast;

use engine_data::v2::engine::DataError;
use engine_data::v2::engine::Datastore;
use engine_data::v2::engine::LiveStream;

/// Bounded so a slow consumer becomes a lag error, not a growing buffer.
const TOPIC_QUEUE: usize = 256;
/// How long an empty topic is kept warm before the LIVE query is dropped.
const IDLE_KEEP: std::time::Duration = std::time::Duration::from_secs(30);

#[derive(Debug, thiserror::Error)]
pub enum HubError {
    #[error("live feed is not enabled on this plane")]
    Unavailable,
    #[error(transparent)]
    Live(DataError),
}

#[derive(Clone)]
pub struct LiveHub {
    inner: Arc<Inner>,
}

pub(super) struct Inner {
    db: Option<Arc<Datastore>>,
    pub(super) topics: HashMap<TopicKey, Topic>,
    generations: AtomicU64,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct TopicKey {
    pub(super) topic: &'static str,
    pub(super) game: String,
}
struct Topic {
    tx: broadcast::Sender<FeedEvent>,
    generation: u64,
    _live: LiveStream,
}

/// The payload sent to feed clients. `seq` orders it within a subscription;
/// the filters and the record are the fields a client already reads.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedEvent {
    pub seq: u64,
    pub topic: &'static str,
    pub game: String,
    pub action: String,
    pub record: Value,
}

pub(super) struct TopicDef {
    pub name: &'static str,
    statement: &'static str,
}

impl TopicDef {
    pub(super) const fn new(name: &'static str, statement: &'static str) -> Self {
        Self { name, statement }
    }
    fn statement(&self) -> String {
        self.statement.to_string()
    }
}

/// Topics a client may subscribe to. A statement is a constant from this module,
/// never a client string, so the LIVE query cannot be steered.
pub(super) const TOPICS: &[TopicDef] = &[
    TopicDef::new(
        "leaderboard",
        "LIVE SELECT * FROM leaderboard WHERE game = $game",
    ),
    TopicDef::new(
        "player_stats",
        "LIVE SELECT * FROM player_stats WHERE game = $game",
    ),
    TopicDef::new(
        "match_record",
        "LIVE SELECT * FROM match_record WHERE game = $game",
    ),
];

pub(super) fn topic(name: &str) -> Option<&'static TopicDef> {
    TOPICS.iter().find(|topic| topic.name == name)
}

impl LiveHub {
    pub fn new(db: Option<Arc<Datastore>>) -> Self {
        Self {
            inner: Arc::new(Inner {
                db,
                topics: HashMap::new(),
                generations: AtomicU64::new(0),
            }),
        }
    }

    pub fn available(&self) -> bool {
        self.inner.db.is_some()
    }

    pub(super) async fn subscribe(
        &self,
        topic: &'static TopicDef,
        game: &str,
    ) -> Result<Subscription, HubError> {
        let db = self.inner.db.as_ref().ok_or(HubError::Unavailable)?;
        let key = TopicKey {
            topic: topic.name,
            game: game.to_string(),
        };
        if let Some(rx) = self.inner.topics.read_sync(&key, |_, existing| {
            if existing.tx.receiver_count() > 0 {
                Some(existing.tx.subscribe())
            } else {
                None
            }
        }) {
            if let Some(rx) = rx {
                return Ok(Subscription {
                    topic: key.topic,
                    rx,
                    last_seq: 0,
                });
            }
            let _ = self.inner.topics.remove_sync(&key);
        }

        let params = std::collections::BTreeMap::from([(
            "game".to_string(),
            serde_json::json!(game.to_string()),
        )]);
        let stream = db
            .live(&topic.statement(), &params)
            .await
            .map_err(HubError::Live)?;
        let (tx, _) = broadcast::channel(TOPIC_QUEUE);
        let rx = tx.subscribe();
        let generation = self.inner.generations.fetch_add(1, Ordering::Relaxed) + 1;

        use scc::hash_map::Entry;
        match self.inner.topics.entry_sync(key.clone()) {
            Entry::Occupied(existing) => {
                let rx = existing.get().tx.subscribe();
                drop(existing);
                drop(stream);
                Ok(Subscription {
                    topic: key.topic,
                    rx,
                    last_seq: 0,
                })
            }
            Entry::Vacant(vacant) => {
                vacant.insert_entry(Topic {
                    tx,
                    generation,
                    _live: stream,
                });
                Ok(Subscription {
                    topic: key.topic,
                    rx,
                    last_seq: 0,
                })
            }
        }
    }

    #[cfg(test)]
    pub(super) async fn topic_count(&self) -> usize {
        self.inner.topics.len()
    }
}
#[derive(Debug)]
pub struct Subscription {
    pub(super) topic: &'static str,
    pub(super) rx: broadcast::Receiver<FeedEvent>,
    pub(super) last_seq: u64,
}
