use super::*;

const TOPIC_QUEUE: usize = 256;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveTopic {
    Users,
    Purchases,
    Subscriptions,
    WalletLedger,
    Leaderboard,
}
impl LiveTopic {
    pub const ALL: &'static [LiveTopic] = &[
        LiveTopic::Users,
        LiveTopic::Purchases,
        LiveTopic::Subscriptions,
        LiveTopic::WalletLedger,
        LiveTopic::Leaderboard,
    ];

    pub fn name(self) -> &'static str {
        match self {
            LiveTopic::Users => "users",
            LiveTopic::Purchases => "purchases",
            LiveTopic::Subscriptions => "subscriptions",
            LiveTopic::WalletLedger => "wallet_ledger",
            LiveTopic::Leaderboard => "leaderboard",
        }
    }

    pub(super) fn statement(self) -> &'static str {
        match self {
            LiveTopic::Users => "LIVE SELECT * FROM users WHERE game_id = $game",
            LiveTopic::Purchases => "LIVE SELECT * FROM purchase WHERE game_id = $game",
            LiveTopic::Subscriptions => "LIVE SELECT * FROM subscription WHERE game_id = $game",
            LiveTopic::WalletLedger => "LIVE SELECT * FROM wallet_ledger WHERE game_id = $game",
            LiveTopic::Leaderboard => "LIVE SELECT * FROM leaderboard WHERE game_id = $game",
        }
    }
}
#[derive(Debug)]
pub enum HubError {
    Unavailable,
    Live(DbError),
}
impl std::fmt::Display for HubError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HubError::Unavailable => f.write_str("no datastore is open"),
            HubError::Live(err) => write!(f, "the live query was refused: {err}"),
        }
    }
}
impl std::error::Error for HubError {}
#[derive(Debug, Clone)]
pub struct FeedEvent {
    pub topic: &'static str,
    pub action: &'static str,
    pub record: String,
    pub seq: u64,
}
pub struct RealtimeHub {
    inner: Arc<Inner>,
}
pub(super) struct Inner {
    db: Option<Arc<Datastore>>,
    pub(super) topics: Mutex<HashMap<TopicKey, Topic>>,
    generations: AtomicU64,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct TopicKey {
    pub(super) topic: &'static str,
    pub(super) game: Uuid,
}
pub(super) struct Topic {
    tx: broadcast::Sender<FeedEvent>,
    pub(super) generation: u64,
}
impl RealtimeHub {
    pub fn new(db: Option<Arc<Datastore>>) -> Self {
        Self {
            inner: Arc::new(Inner {
                db,
                topics: Mutex::new(HashMap::new()),
                generations: AtomicU64::new(0),
            }),
        }
    }

    pub async fn subscribe(&self, topic: LiveTopic, game: Uuid) -> Result<Subscription, HubError> {
        let db = self.inner.db.clone().ok_or(HubError::Unavailable)?;
        let key = TopicKey {
            topic: topic.name(),
            game,
        };
        let mut topics = self.inner.topics.lock().await;
        if let Some(existing) = topics.get(&key) {
            if existing.tx.receiver_count() > 0 {
                return Ok(Subscription {
                    topic: key.topic,
                    rx: existing.tx.subscribe(),
                    last_seq: 0,
                });
            }
            topics.remove(&key);
        }
        let params = std::collections::BTreeMap::from([(
            "game".to_string(),
            serde_json::json!(game.to_string()),
        )]);
        let stream = db
            .live(topic.statement(), &params)
            .await
            .map_err(HubError::Live)?;
        let (tx, _) = broadcast::channel(TOPIC_QUEUE);
        let rx = tx.subscribe();
        let generation = self.inner.generations.fetch_add(1, Ordering::Relaxed) + 1;
        topics.insert(
            key.clone(),
            Topic {
                tx: tx.clone(),
                generation,
            },
        );
        tokio::spawn(run_topic(
            Arc::clone(&self.inner),
            key,
            generation,
            tx,
            stream,
        ));
        Ok(Subscription {
            topic: topic.name(),
            rx,
            last_seq: 0,
        })
    }

    #[cfg(test)]
    pub(super) async fn topic_count(&self) -> usize {
        self.inner.topics.lock().await.len()
    }
}
#[derive(Debug)]
pub struct Subscription {
    pub(super) topic: &'static str,
    pub(super) rx: broadcast::Receiver<FeedEvent>,
    pub(super) last_seq: u64,
}
