//! The control plane's half of the host's event mirror. The sim queues listed
//! guest messages and its own desync evidence on the bus; this drains them into
//! rows and ages the history out. Nothing here may slow the sim: its queue is
//! bounded and drops its oldest row, and the `seq` gaps a reader sees are how
//! that loss becomes visible instead of silent.
//!
//! The same pump is also the quest progress sink: the host bridges each seat to
//! the user id its client claimed, and a mirrored event whose name is a visible
//! quest counter credits that user — the same counters the server-key ingest
//! door writes, so a quest reads one number no matter which door moved it.

use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use engine_bus::{RoomSnapshot, SeatUpdate, SimEvent};
use uuid::Uuid;

use crate::ApiState;
use crate::repositories::models::{
    DesyncReport, GameplayEvent, MatchHistory, QuestDefinition, RoomRegistry,
};
use crate::services::quest::catalog::{open_seasons, visible};

const PUMP_INTERVAL: Duration = Duration::from_millis(250);
const SWEEP_INTERVAL: Duration = Duration::from_secs(3600);
/// How often the pump re-reads the quest catalog for its counter → quest map.
/// A definition created or activated mid-flight takes up to this long to start
/// counting mirrored events — the same laziness the player-facing list already
/// has, and cheap next to one query per event.
const COUNTER_REFRESH_TICKS: u32 = 240;
/// The engine's own event: two checksums that disagreed, little-endian.
const DESYNC_EVENT: &str = "desync";
const CHECKSUM_PAIR_LEN: usize = 16;

/// The seat → user bridges this pump has seen. The bus is in-process, so the
/// host that sent the bridge and this map share a lifetime: a lost process
/// loses both at once, and the first join after a restart re-seats everyone.
/// Seats are never reused inside a room's life, and a `Closed` ownership row
/// drops the room's bridges before its name can be reused — so a mapping can
/// never credit the wrong user.
#[derive(Default)]
struct SeatBridges(HashMap<(String, i64), Uuid>);

impl SeatBridges {
    fn apply(&mut self, seat: SeatUpdate) {
        match seat.user_id.and_then(|id| Uuid::parse_str(&id).ok()) {
            Some(user_id) => {
                self.0.insert((seat.room, seat.seat), user_id);
            }
            None => {
                self.0.remove(&(seat.room, seat.seat));
            }
        }
    }

    /// A disposed room's name is free for a new room to take; its seats start
    /// counting at 1 again, so the old bridges must go with it.
    fn drop_room(&mut self, room: &str) {
        self.0.retain(|(name, _), _| name != room);
    }

    fn resolve(&self, event: &SimEvent) -> Option<Uuid> {
        if event.target == 0 {
            return None;
        }
        self.0.get(&(event.room.clone(), event.target)).copied()
    }
}

/// The visible quest catalog keyed by counter name, refreshed on a timer so the
/// per-event path is memory reads only. `None` until the first successful
/// refresh; a pump that starts before the datastore is up just waits for the
/// next beat.
struct QuestCounters {
    by_counter: HashMap<String, Vec<QuestDefinition>>,
    ticks_since_refresh: u32,
}

impl QuestCounters {
    fn new() -> Self {
        Self {
            by_counter: HashMap::new(),
            ticks_since_refresh: COUNTER_REFRESH_TICKS,
        }
    }

    /// Rebuild the counter map from the quests that run right now. A failure
    /// keeps the previous map: stale counters lag an operator's change, an
    /// empty one would silently stop crediting everything.
    async fn refresh(&mut self, state: &ApiState) {
        self.ticks_since_refresh = 0;
        let Some(queries) = state.q.as_ref() else {
            return;
        };
        let now = Utc::now();
        let Ok(seasons) = open_seasons(queries, now).await else {
            tracing::warn!("[control] quest counter refresh: seasons unavailable");
            return;
        };
        let definitions = match crate::services::quest::owned_definitions(queries, state).await {
            Ok(definitions) => definitions,
            Err(err) => {
                tracing::warn!("[control] quest counter refresh failed: {err:?}");
                return;
            }
        };
        self.by_counter = visible(&definitions, &seasons, now).into_iter().fold(
            HashMap::new(),
            |mut map, quest| {
                map.entry(quest.goal_kind.clone())
                    .or_default()
                    .push(quest.clone());
                map
            },
        );
    }

    fn due(&self) -> bool {
        self.ticks_since_refresh >= COUNTER_REFRESH_TICKS
    }
}

/// The lifecycle name a host reported, as the registry stores it. Exhaustive on
/// purpose: a fifth phase should fail the build here rather than be filed under
/// some name no reader would think to query for.
fn phase_name(phase: engine_bus::RoomPhase) -> &'static str {
    match phase {
        engine_bus::RoomPhase::Created => "created",
        engine_bus::RoomPhase::Active => "active",
        engine_bus::RoomPhase::Hibernating => "hibernating",
        engine_bus::RoomPhase::Closed => "closed",
    }
}

/// The host reports wall seconds because an `Instant` cannot survive a restart.
fn stamp(secs: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(secs, 0).unwrap_or_else(Utc::now)
}

/// Score a result the room reported for itself.
///
/// The players are taken from the report, not from the realtime registry: a
/// room that has just closed is gone from the registry, and asking it who was
/// seated would answer "nobody" for every match. The room's own list is the
/// authority here because the node that held it resolved those ids from the
/// seats it was given, which is the same set the registry would have returned
/// a moment earlier.
///
/// Everything after that is `record_result`'s own path — the same balance
/// checks, the same counters, the same rules and webhook. A guest's claim is
/// worth no more than an HTTP caller's.
async fn score_reported_result(state: &ApiState, finished: &engine_bus::MatchFinished) {
    let outcome = match crate::services::match_result::reported::decode(&finished.result) {
        Ok(outcome) => outcome,
        Err(problem) => {
            // A result the control plane cannot read is dropped, loudly. The
            // room is already gone; there is nowhere to return it to.
            tracing::warn!(
                "[control] room '{}' reported a result that could not be read: {problem}",
                finished.room
            );
            return;
        }
    };
    let seats: std::collections::HashMap<Uuid, String> = outcome
        .participants
        .iter()
        .map(|part| (part.user_id, String::new()))
        .collect();
    let request = outcome.into_request();
    let service = crate::services::match_result::MatchResultService::new();
    match service.apply(state, Uuid::new_v4(), request, seats).await {
        Ok(_) => tracing::info!("[control] room '{}' result scored", finished.room),
        Err(error) => tracing::warn!(
            "[control] room '{}' result refused: {:?}",
            finished.room,
            error
        ),
    }
}

fn checksum_pair(payload: &[u8]) -> Option<(u64, u64)> {
    if payload.len() != CHECKSUM_PAIR_LEN {
        return None;
    }
    let word = |at: usize| {
        let mut a = [0u8; 8];
        a.copy_from_slice(&payload[at..at + 8]);
        u64::from_le_bytes(a)
    };
    Some((word(0), word(8)))
}

pub async fn pump_events(state: ApiState) {
    let Some(q) = state.q.clone() else {
        tracing::info!("[control] no datastore; mirrored events are dropped on the floor");
        return;
    };
    // One pump owns the queue. A second consumer would split it, and every
    // event would land in whichever task happened to be polling.
    let rx = state.bus.events_rx.clone();
    let matches_rx = state.bus.matches_rx.clone();
    let snapshots_rx = state.bus.snapshots_rx.clone();
    let fetches_rx = state.bus.fetches_rx.clone();
    let ownership_rx = state.bus.ownership_rx.clone();
    let seats_rx = state.bus.seats_rx.clone();
    let mut seats = SeatBridges::default();
    let mut counters = QuestCounters::new();
    let mut ticker = tokio::time::interval(PUMP_INTERVAL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        counters.ticks_since_refresh += 1;
        if counters.due() {
            counters.refresh(&state).await;
        }
        // A seat bridge is a claim, not a line item: the newest one for a
        // (room, seat) is simply what the map holds, and a release removes it.
        while let Ok(seat) = seats_rx.try_recv() {
            seats.apply(seat);
        }
        while let Ok(event) = rx.try_recv() {
            let desync = if event.event.as_ref() == DESYNC_EVENT {
                checksum_pair(&event.payload)
            } else {
                None
            };
            let stored = match desync {
                Some((expected, client)) => {
                    let report = DesyncReport {
                        id: Uuid::new_v4(),
                        room: event.room,
                        player_id: event.target,
                        tick: event.tick,
                        expected_checksum: format!("{expected:016x}"),
                        client_checksum: format!("{client:016x}"),
                        create_time: Utc::now(),
                    };
                    let summary = serde_json::json!({
                        "report_id": report.id,
                        "room": report.room,
                        "player_id": report.player_id,
                        "tick": report.tick,
                        "expected_checksum": report.expected_checksum,
                        "client_checksum": report.client_checksum,
                    });
                    let key = report.id.to_string();
                    let stored = q.record_desync_report(&report).await;
                    if stored.is_ok() {
                        crate::services::webhook::emit(&state, "report.created", &key, summary)
                            .await;
                    }
                    stored
                }
                None => {
                    let credited = credit_mirrored_event(&state, &seats, &counters, &event).await;
                    if let Err(err) = credited {
                        tracing::warn!(
                            "[control] mirrored event '{}' could not be credited: {err}",
                            event.event
                        );
                    }
                    q.record_gameplay_event(&GameplayEvent {
                        id: Uuid::new_v4(),
                        room: event.room,
                        tick: event.tick,
                        seq: event.seq as i64,
                        player_id: event.target,
                        event: event.event.to_string(),
                        payload: event.payload.to_vec(),
                        create_time: Utc::now(),
                    })
                    .await
                }
            };
            if let Err(err) = stored {
                tracing::warn!("[control] dropped a mirrored event: {err}");
            }
        }
        // A finished match is one row per room, and the room is already gone by
        // the time it arrives: losing it is a hole in the ledger, not a stall.
        while let Ok(finished) = matches_rx.try_recv() {
            // A round that reported its own result is scored here, before the
            // history row takes ownership of the bytes. The guest said who won;
            // the control plane decides whether that is worth scoring, through
            // the same checks that guard the REST endpoint.
            if !finished.result.is_empty() {
                score_reported_result(&state, &finished).await;
            }
            if let Err(err) = q
                .record_match(&MatchHistory {
                    id: Uuid::new_v4(),
                    game_id: finished.game_id,
                    room: finished.room,
                    end_reason: finished.end_reason,
                    started_at: stamp(finished.started_at_unix),
                    ended_at: stamp(finished.ended_at_unix),
                    ticks: finished.ticks,
                    peak_players: finished.peak_players,
                    snapshot_seq: finished.snapshot_seq,
                    result: finished.result,
                    create_time: Utc::now(),
                })
                .await
            {
                tracing::warn!("[control] dropped a finished match: {err}");
            }
        }
        // A snapshot is the room's current state, so the queue holds at most one
        // per room and an older one is worth nothing — superseding it is correct,
        // and writing the bytes a node is about to lose is the whole point.
        while let Ok(saved) = snapshots_rx.try_recv() {
            if let Err(err) = q
                .save_room_snapshot(&saved.room, &saved.backend, &saved.bytes, 0, saved.seq)
                .await
            {
                tracing::warn!("[control] dropped a room snapshot: {err}");
            }
        }
        // A host spawning a room it has never seen asks for the state an earlier
        // run saved. It is waiting on the answer, so a lookup that failed is
        // answered with `None` rather than left to time out: "start empty" is
        // the same outcome the host reaches on its own, and it reaches it sooner.
        while let Ok(fetch) = fetches_rx.try_recv() {
            let found = match q.load_room_snapshot(&fetch.room).await {
                Ok(Some(row)) => Some(RoomSnapshot {
                    room: row.room_name,
                    backend: row.backend,
                    seq: row.snapshot_seq,
                    bytes: row.snapshot_bytes,
                }),
                Ok(None) => None,
                Err(err) => {
                    tracing::warn!(
                        "[control] snapshot fetch for '{}' failed: {err}",
                        fetch.room
                    );
                    None
                }
            };
            let _ = fetch.reply.send(found);
        }
        // An ownership row is the host's heartbeat for one room: the same name
        // refreshes its own row every beat, so the table holds one row per live
        // room and `update_time` is how long the answer has been true. A released
        // room's row is deleted rather than marked, because a reader that has to
        // remember to filter a status string is a reader that will forget.
        while let Ok(row) = ownership_rx.try_recv() {
            let stored = if row.state == engine_bus::RoomPhase::Closed {
                // The name is about to be free: its seat bridges must not
                // survive it, or the next room's seat 1 would credit the old
                // room's player.
                seats.drop_room(&row.room);
                q.release_room(&row.room).await
            } else {
                q.claim_room(&RoomRegistry {
                    room_id: row.room.clone(),
                    node_id: row.node,
                    game_id: row.game,
                    state: phase_name(row.state).to_string(),
                    peers: row.peers,
                    tick: row.tick,
                    snapshot_seq: row.snapshot_seq,
                    update_time: Utc::now(),
                })
                .await
            };
            if let Err(err) = stored {
                tracing::warn!(
                    "[control] dropped the registry row for '{}': {err}",
                    row.room
                );
            }
        }
    }
}

/// Credits one mirrored gameplay event toward quest progress, if it names a
/// visible quest counter and its seat is bridged to a user. One event is one
/// increment — the payload is game-defined bytes the control plane does not
/// parse, so a game that wants weighted counters sends the weight through the
/// server-key ingest door instead. A seat whose claimed user does not exist as
/// an account is dropped: counters are keyed by real accounts, and a bogus
/// claim must not mint rows.
async fn credit_mirrored_event(
    state: &ApiState,
    seats: &SeatBridges,
    counters: &QuestCounters,
    event: &SimEvent,
) -> Result<(), String> {
    let Some(quests) = counters.by_counter.get(event.event.as_ref()) else {
        return Ok(());
    };
    let Some(user_id) = seats.resolve(event) else {
        return Ok(());
    };
    let Some(queries) = state.q.as_ref() else {
        return Ok(());
    };
    if queries.get_user_by_id(user_id).await.is_err() {
        return Ok(());
    }
    let visible: Vec<&crate::repositories::models::QuestDefinition> = quests.iter().collect();
    crate::services::quest::events::credit_counter(
        state,
        queries,
        user_id,
        &event.event,
        1,
        &visible,
    )
    .await
    .map(|_| ())
    .map_err(|err| err.to_string())
}

/// History nobody reads inside the retention window will not be read later, and
/// these rows are the bulk of what the engine writes. Each table ages on its own
/// window: match summaries outlive the event stream they came from.
pub async fn sweep_match_history(state: ApiState) {
    let Some(q) = state.q.clone() else {
        return;
    };
    let mut ticker = tokio::time::interval(SWEEP_INTERVAL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        let cutoff = |days: u32| Utc::now() - chrono::Duration::days(i64::from(days.max(1)));
        if let Err(err) = q
            .prune_event_history(cutoff(state.game.gameplay.event_ttl_days))
            .await
        {
            tracing::warn!("[control] event history prune failed: {err}");
        }
        if let Err(err) = q
            .prune_match_history(cutoff(state.game.gameplay.match_history_ttl_days))
            .await
        {
            tracing::warn!("[control] match history prune failed: {err}");
        }
    }
}

#[cfg(test)]
mod seat_credit_tests {
    use super::*;
    use engine_bus::Bus;
    use engine_config::{ControlConfig, GameConfig, SessionConfig};
    use std::path::Path;

    /// Bridges are keyed by `(room, seat)` and a room's disposal clears the
    /// whole room: seats restart at 1 for the next room of the same name, and
    /// a survivor of the old one would credit the wrong player.
    #[test]
    fn a_bridge_lives_from_join_until_leave_or_room_close() {
        let user = Uuid::now_v7();
        let mut bridges = SeatBridges::default();
        bridges.apply(SeatUpdate {
            room: "arena".into(),
            seat: 1,
            user_id: Some(user.to_string()),
        });
        bridges.apply(SeatUpdate {
            room: "arena".into(),
            seat: 2,
            user_id: Some("not a uuid".into()),
        });
        assert_eq!(
            bridges.resolve(&SimEvent {
                room: "arena".into(),
                tick: 1,
                seq: 1,
                target: 1,
                event: "coins".into(),
                payload: vec![].into(),
            }),
            Some(user)
        );
        // A garbage claim bridges nothing, and neither does a broadcast.
        assert_eq!(
            bridges.resolve(&SimEvent {
                room: "arena".into(),
                tick: 1,
                seq: 2,
                target: 2,
                event: "coins".into(),
                payload: vec![].into(),
            }),
            None
        );
        assert_eq!(
            bridges.resolve(&SimEvent {
                room: "arena".into(),
                tick: 1,
                seq: 3,
                target: 0,
                event: "coins".into(),
                payload: vec![].into(),
            }),
            None
        );
        bridges.apply(SeatUpdate {
            room: "arena".into(),
            seat: 1,
            user_id: None,
        });
        assert_eq!(
            bridges.resolve(&SimEvent {
                room: "arena".into(),
                tick: 1,
                seq: 4,
                target: 1,
                event: "coins".into(),
                payload: vec![].into(),
            }),
            None,
            "the leave cleared the bridge"
        );
        bridges.apply(SeatUpdate {
            room: "arena".into(),
            seat: 3,
            user_id: Some(user.to_string()),
        });
        bridges.drop_room("arena");
        assert!(
            bridges.0.is_empty(),
            "the closed room took its bridges with it"
        );
    }

    /// The full loop: a seated guest's mirrored `coins` events move the same
    /// `player_stat` counter the ingest door writes, and the quest keyed to it
    /// completes the moment the second increment crosses the goal.
    #[tokio::test]
    async fn a_mirrored_event_credits_the_seated_user_toward_a_quest() {
        let bus = Bus::new(64, 64);
        let control = ControlConfig {
            session: SessionConfig {
                database_endpoint: "mem://".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let game = GameConfig {
            name: "tank-2d".into(),
            ..Default::default()
        };
        let state = crate::ApiState::new(Path::new("."), &game, &control, bus.clone(), 0).await;
        let queries = state.q.clone().expect("mem:// opens a datastore");
        let db = state.db.as_deref().expect("the mem datastore");
        crate::db::schema::ensure_schema(db)
            .await
            .expect("base schema");
        crate::db::migrations::apply_migrations(db, crate::db::migrations::MIGRATIONS)
            .await
            .expect("schema migrations");

        let user = Uuid::now_v7();
        queries
            .create_user_bare(user, "mirrored-player")
            .await
            .expect("seed the account the seat claims");
        let now = Utc::now();
        queries
            .create_quest_definition(&crate::repositories::models::QuestDefinition {
                id: "coin-hoarder".into(),
                game_id: None,
                title: "Coin hoarder".into(),
                goal_kind: "coins".into(),
                goal_amount: 2,
                reward: serde_json::json!({ "coins": 10 }),
                season: None,
                requires_product: None,
                active_from: now - chrono::Duration::hours(1),
                active_to: now + chrono::Duration::hours(1),
                create_time: now,
                update_time: now,
            })
            .await
            .expect("seed the quest");

        bus.push_seat(SeatUpdate {
            room: "arena".into(),
            seat: 1,
            user_id: Some(user.to_string()),
        });
        for seq in 1..=2u64 {
            bus.push_event(SimEvent {
                room: "arena".into(),
                tick: seq as i64,
                seq,
                target: 1,
                event: "coins".into(),
                payload: vec![].into(),
            });
        }

        let pump = tokio::spawn(crate::pump_events(state.clone()));
        let mut credited = false;
        for _ in 0..100 {
            if queries
                .get_player_stat(user, "coins")
                .await
                .ok()
                .flatten()
                .is_some_and(|row| row.value >= 2)
            {
                credited = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        pump.abort();
        assert!(credited, "the pump never credited the second event");

        let rows = queries
            .list_player_quests(user)
            .await
            .expect("progress rows");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].quest_key, "coin-hoarder");
        assert_eq!(rows[0].state, "completed", "the goal was crossed");
        assert_eq!(rows[0].progress, 2);
    }

    /// A mirrored event from a seat nobody bridged moves nothing: without the
    /// bridge the control plane cannot know which user to credit, and guessing
    /// would put one player's progress on another's account.
    #[tokio::test]
    async fn an_unbridged_seat_credits_nothing() {
        let bus = Bus::new(64, 64);
        let control = ControlConfig {
            session: SessionConfig {
                database_endpoint: "mem://".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let game = GameConfig {
            name: "tank-2d".into(),
            ..Default::default()
        };
        let state = crate::ApiState::new(Path::new("."), &game, &control, bus.clone(), 0).await;
        let queries = state.q.clone().expect("mem:// opens a datastore");
        let db = state.db.as_deref().expect("the mem datastore");
        crate::db::schema::ensure_schema(db)
            .await
            .expect("base schema");
        crate::db::migrations::apply_migrations(db, crate::db::migrations::MIGRATIONS)
            .await
            .expect("schema migrations");
        let user = Uuid::now_v7();
        queries
            .create_user_bare(user, "unbridged-player")
            .await
            .expect("seed the account");
        // The account is seated somewhere — just not where the event came from.
        bus.push_seat(SeatUpdate {
            room: "arena".into(),
            seat: 9,
            user_id: Some(user.to_string()),
        });
        let now = Utc::now();
        queries
            .create_quest_definition(&crate::repositories::models::QuestDefinition {
                id: "coin-hoarder".into(),
                game_id: None,
                title: "Coin hoarder".into(),
                goal_kind: "coins".into(),
                goal_amount: 2,
                reward: serde_json::json!({ "coins": 10 }),
                season: None,
                requires_product: None,
                active_from: now - chrono::Duration::hours(1),
                active_to: now + chrono::Duration::hours(1),
                create_time: now,
                update_time: now,
            })
            .await
            .expect("seed the quest");
        bus.push_event(SimEvent {
            room: "arena".into(),
            tick: 1,
            seq: 1,
            target: 4,
            event: "coins".into(),
            payload: vec![].into(),
        });

        let pump = tokio::spawn(crate::pump_events(state.clone()));
        tokio::time::sleep(Duration::from_millis(700)).await;
        pump.abort();

        let stats = queries.list_player_stats(user).await.expect("stat query");
        assert!(
            stats.is_empty(),
            "the event on seat 4 credited seat 9's user"
        );
    }
}
