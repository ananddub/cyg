//! A tracing layer that mirrors every formatted event into (a) a bounded
//! broadcast channel for the dashboard's live log view and (b) a ring buffer
//! the `logs/recent` endpoint reads. Both sinks shed load instead of ever
//! blocking the thread that logged.

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

use engine_dashboard::models::log::LogLine;
use tokio::sync::broadcast;
use tracing::Event;
use tracing_subscriber::Layer;

const RING_CAPACITY: usize = 1000;
const LIVE_CAPACITY: usize = 256;

struct Sinks {
    ring: Mutex<VecDeque<LogLine>>,
    live: broadcast::Sender<LogLine>,
    dropped: std::sync::atomic::AtomicUsize,
}

fn sinks() -> &'static Sinks {
    static SINKS: OnceLock<Sinks> = OnceLock::new();
    SINKS.get_or_init(|| Sinks {
        ring: Mutex::new(VecDeque::with_capacity(RING_CAPACITY)),
        live: broadcast::channel(LIVE_CAPACITY).0,
        dropped: std::sync::atomic::AtomicUsize::new(0),
    })
}

/// Install through the registry, beside the normal fmt layer.
#[derive(Clone, Copy, Default)]
pub struct FeedLayer;

impl<S: tracing::Subscriber> Layer<S> for FeedLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);
        let line = LogLine {
            at_ms: chrono::Utc::now().timestamp_millis(),
            level: event.metadata().level().to_string(),
            target: event.metadata().target().to_owned(),
            message: visitor.into_message(),
        };
        {
            let mut ring = sinks().ring.lock().expect("the log ring never panics");
            if ring.len() == RING_CAPACITY {
                ring.pop_front();
            }
            ring.push_back(line.clone());
        }
        if sinks().live.send(line).is_err() {
            sinks()
                .dropped
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

pub fn layer() -> FeedLayer {
    FeedLayer
}

/// The live stream; a receiver that falls behind sees `Lagged` and skips.
pub fn subscribe() -> broadcast::Receiver<LogLine> {
    sinks().live.subscribe()
}

/// The newest `limit` lines, oldest first.
pub fn recent(limit: usize) -> Vec<LogLine> {
    let ring = sinks().ring.lock().expect("the log ring never panics");
    let start = ring.len().saturating_sub(limit);
    ring.iter().skip(start).cloned().collect()
}

pub fn dropped_live_lines() -> usize {
    sinks().dropped.load(std::sync::atomic::Ordering::Relaxed)
}

#[derive(Default)]
struct MessageVisitor {
    parts: Vec<String>,
}

impl tracing::field::Visit for MessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.parts.insert(0, format!("{value:?}"));
        } else {
            self.parts.push(format!("{}={value:?}", field.name()));
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.parts.insert(0, value.to_owned());
        } else {
            self.parts.push(format!("{}={value}", field.name()));
        }
    }
}

impl MessageVisitor {
    fn into_message(self) -> String {
        self.parts.join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tracing_subscriber::prelude::*;

    #[test]
    fn a_full_live_channel_drops_instead_of_blocking() {
        // A channel with no receiver: sends are instant and counted.
        let started = std::time::Instant::now();
        let feed = subscribe();
        drop(feed);
        tracing::subscriber::with_default(tracing_subscriber::registry().with(layer()), || {
            for i in 0..(LIVE_CAPACITY * 4) {
                tracing::info!("flood {i}");
            }
        });
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "an unread feed must never block the logging thread"
        );
        assert!(dropped_live_lines() > 0, "the flood is counted as dropped");

        let lines = recent(10);
        assert_eq!(lines.len(), 10);
        assert!(lines.last().expect("newest line").message.contains("flood"));
    }

    #[tokio::test]
    async fn a_receiver_sees_lines_and_the_ring_keeps_the_tail() {
        let mut feed = subscribe();
        tracing::subscriber::with_default(tracing_subscriber::registry().with(layer()), || {
            tracing::warn!("watched line");
        });
        // The sinks are global, so a parallel test may flood the channel first;
        // the assertion is that the watched line arrives, not that it is first.
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let mut seen = None;
        while std::time::Instant::now() < deadline {
            match tokio::time::timeout(Duration::from_millis(200), feed.recv()).await {
                Ok(Ok(line)) if line.message == "watched line" => {
                    seen = Some(line);
                    break;
                }
                Ok(Ok(_)) | Ok(Err(broadcast::error::RecvError::Lagged(_))) => continue,
                _ => break,
            }
        }
        let line = seen.expect("the live feed delivers the watched line");
        assert_eq!(line.level, "WARN");
    }
}
