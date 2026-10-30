use std::collections::HashMap;
use std::time::{Duration, Instant};

use chrono::Utc;
use tokio::task::JoinSet;
use uuid::Uuid;

use crate::ApiState;
use crate::infra::liveops;
use crate::services::deps::q;

use super::deliver::{self, Verdict};

/// Drains the outbox. A request queues the fact it made true and leaves the
/// network to this loop, so an endpoint that takes ten seconds to answer cannot
/// slow a player down.
pub async fn delivery_loop(state: ApiState) {
    let config = liveops::config();
    let timeout = Duration::from_millis(config.webhook_timeout_ms);
    let Ok(client) = reqwest::Client::builder().timeout(timeout).build() else {
        tracing::warn!("[webhook] no HTTP client; queued facts stay queued");
        return;
    };
    let concurrency = config.webhook_concurrency;
    let mut beat = tokio::time::interval(Duration::from_millis(config.webhook_poll_ms));
    beat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut in_flight = JoinSet::new();
    let mut circuit = Circuit {
        threshold: config.webhook_circuit_trips,
        cooldown_ms: i64::try_from(config.webhook_circuit_cooldown_ms).unwrap_or(i64::MAX),
        trips: HashMap::new(),
    };

    loop {
        beat.tick().await;
        let Ok(queries) = q(&state).await else {
            continue;
        };
        // A row still marked `sending` twice the request timeout ago was taken by
        // a process that never came back for it.
        let _ = queries
            .requeue_stalled(deliver::stalled_before(Utc::now(), timeout))
            .await;

        while let Some(done) = in_flight.try_join_next() {
            let Ok(Some((endpoint_id, Some(verdict)))) = done else {
                continue;
            };
            if circuit.record(endpoint_id, verdict) {
                // Nothing is owed to this endpoint until it has been left alone.
                let _ = queries
                    .defer_endpoint(endpoint_id, Utc::now() + circuit.pause())
                    .await;
            }
        }

        if in_flight.len() >= concurrency {
            continue;
        }
        let room = (concurrency - in_flight.len()) as i64;
        let due = match queries.due_deliveries(room * 4).await {
            Ok(rows) => rows,
            Err(err) => {
                tracing::warn!("[webhook] the outbox could not be read: {err}");
                continue;
            }
        };
        for row in due {
            if in_flight.len() >= concurrency {
                break;
            }
            let endpoint = match queries.webhook_endpoint(row.endpoint_id).await {
                Ok(Some(endpoint)) => endpoint,
                Ok(None) => {
                    // The operator unregistered it. The row stays behind as
                    // evidence of a fact nobody can be paid for any more.
                    let _ = queries
                        .abandon_delivery(row.id, "the endpoint is no longer registered")
                        .await;
                    continue;
                }
                Err(_) => continue,
            };
            if circuit.paused(&endpoint.id) {
                continue;
            }
            let queries = queries.clone();
            let client = client.clone();
            in_flight.spawn(async move {
                let verdict = deliver::attempt(&queries, &client, &endpoint, &row).await;
                Some((row.endpoint_id, verdict))
            });
        }
    }
}

/// An endpoint that fails this many times in a row is left alone for a cooldown,
/// so one dead receiver cannot eat the worker's whole budget.
struct Circuit {
    threshold: i64,
    cooldown_ms: i64,
    trips: HashMap<Uuid, (i64, Instant)>,
}

impl Circuit {
    fn paused(&self, endpoint_id: &Uuid) -> bool {
        self.trips
            .get(endpoint_id)
            .is_some_and(|(_, open)| *open > Instant::now())
    }

    fn pause(&self) -> chrono::Duration {
        chrono::Duration::milliseconds(self.cooldown_ms)
    }

    /// Returns true when this verdict just opened the circuit. A 4xx is a
    /// decision the endpoint made soberly, so it neither trips nor clears it.
    fn record(&mut self, endpoint_id: Uuid, verdict: Verdict) -> bool {
        match verdict {
            Verdict::Delivered => {
                self.trips.remove(&endpoint_id);
                false
            }
            Verdict::Rejected => false,
            Verdict::Failing => {
                let trips = self.trips.get(&endpoint_id).map_or(0, |(t, _)| *t) + 1;
                if trips >= self.threshold {
                    self.trips
                        .insert(endpoint_id, (0, Instant::now() + self.pause_std()));
                    true
                } else {
                    self.trips.insert(endpoint_id, (trips, Instant::now()));
                    false
                }
            }
        }
    }

    fn pause_std(&self) -> Duration {
        Duration::from_millis(self.cooldown_ms as u64)
    }
}
