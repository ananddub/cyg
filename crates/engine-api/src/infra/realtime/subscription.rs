use super::*;

impl Subscription {
    pub fn topic(&self) -> &'static str {
        self.topic
    }

    pub async fn recv(&mut self) -> Option<FeedEvent> {
        match self.rx.recv().await {
            Ok(event) => {
                self.last_seq = event.seq;
                Some(event)
            }
            Err(broadcast::error::RecvError::Lagged(_)) => Some(FeedEvent {
                topic: self.topic,
                action: "resync",
                record: String::new(),
                seq: self.last_seq,
            }),
            Err(broadcast::error::RecvError::Closed) => None,
        }
    }
}
pub(super) async fn run_topic(
    inner: Arc<Inner>,
    key: TopicKey,
    generation: u64,
    tx: broadcast::Sender<FeedEvent>,
    mut stream: LiveStream,
) {
    let mut seq = 0u64;
    loop {
        tokio::select! {
            _ = tx.closed() => break,
            change = stream.next_change() => match change {
                Some(Ok(change)) => {
                    seq += 1;
                    let event = FeedEvent {
                        topic: key.topic,
                        action: change.action.as_str(),
                        record: record_key(&change.row),
                        seq,
                    };
                    let _ = tx.send(event);
                }
                Some(Err(err)) => {
                    tracing::warn!("[realtime] {} feed for game {}: {err}", key.topic, key.game);
                    break;
                }
                None => break,
            },
        }
    }
    retire(&inner, &key, generation).await;
}
async fn retire(inner: &Inner, key: &TopicKey, generation: u64) {
    let mut topics = inner.topics.lock().await;
    if topics
        .get(key)
        .is_some_and(|topic| topic.generation == generation)
    {
        topics.remove(key);
    }
}
fn record_key(row: &serde_json::Value) -> String {
    let id = row
        .get("id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    match id.split_once(':') {
        Some((_, key)) => crate::db::exec::strip_key(key),
        None => id.to_string(),
    }
}
