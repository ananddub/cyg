//! Single-node matchmaking queue.
//!
//! A ticket is one party. The queue groups tickets by room/mode/region and
//! forms a match only when the party sizes exactly fill the requested cap, so
//! a party is never split. Seats are minted lazily by the controller; a
//! matched ticket without seats is still safe to read and cannot produce two
//! handoffs.
use std::sync::Arc;

use crate::services::plane::Plane;

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use auto_di::singleton;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ApiError;
use crate::infra::seats::{Seat, SeatRegistry};

const DEFAULT_MAX_PLAYERS: usize = 2;
const MAX_MAX_PLAYERS: usize = 64;
const MAX_PARTY: usize = 16;
const DEFAULT_TTL_MS: u64 = 60_000;
const MAX_TTL_MS: u64 = 10 * 60_000;
const BASE_MMR_WINDOW: f64 = 100.0;
const MMR_WINDOW_STEP_MS: u64 = 5_000;
const MMR_WINDOW_STEP: f64 = 50.0;
const MAX_MMR_WINDOW: f64 = 500.0;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueJoin {
    pub room: String,
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub region: String,
    #[serde(default)]
    pub mmr: f64,
    /// How many players this ticket covers. Account ids never cross this
    /// boundary: the caller receives that many seat tokens and distributes
    /// them, so a client cannot claim another account's ticket.
    #[serde(default)]
    pub party_size: Option<usize>,
    pub max_players: Option<usize>,
    pub ttl_ms: Option<u64>,
}

#[derive(poem_openapi::Object, Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueTicketView {
    pub ticket_id: String,
    pub status: String,
    pub room: String,
    pub mode: String,
    pub region: String,
    pub party_size: usize,
    pub max_players: usize,
    pub position: usize,
    pub waited_ms: u64,
    pub eta_ms: Option<u64>,
    pub match_id: Option<String>,
    pub seats: Vec<QueueSeatView>,
    pub expires_at_ms: u64,
}

#[derive(poem_openapi::Object, Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueSeatView {
    pub name: String,
    pub session_id: String,
    pub room_id: String,
    pub reconnection_token: String,
    pub seat_token: String,
    pub expires_at: u64,
}

impl QueueSeatView {
    fn from_seat(seat: &Seat) -> Self {
        Self {
            name: seat.room.clone(),
            session_id: seat.id.clone(),
            room_id: seat.room.clone(),
            reconnection_token: seat.token.clone(),
            seat_token: seat.token.clone(),
            expires_at: seat.expires_at,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Status {
    Queued,
    Matched { match_id: String, room: String },
    Cancelled,
    Expired,
}

#[derive(Clone, Debug)]
struct Ticket {
    id: String,
    room: String,
    mode: String,
    region: String,
    mmr: f64,
    party_size: usize,
    max_players: usize,
    created_ms: u64,
    expires_ms: u64,
    status: Status,
    seats: Option<Vec<Seat>>,
}

#[derive(Default)]
struct Inner {
    tickets: HashMap<String, Ticket>,
}

pub struct QueueService {
    plane: Arc<Plane>,
    inner: Mutex<Inner>,
}

#[singleton]
impl QueueService {
    pub fn new(plane: Arc<Plane>) -> Self {
        Self {
            plane,
            inner: Mutex::new(Inner::default()),
        }
    }

    pub fn join(&self, req: QueueJoin) -> Result<QueueTicketView, ApiError> {
        self.join_at(req, now_ms())
    }

    fn join_at(&self, req: QueueJoin, now: u64) -> Result<QueueTicketView, ApiError> {
        let room = clean_room(&req.room)?;
        let mode = clean_label(&req.mode, "mode")?;
        let region = clean_label(&req.region, "region")?;
        let max_players = req
            .max_players
            .unwrap_or(DEFAULT_MAX_PLAYERS)
            .clamp(1, MAX_MAX_PLAYERS);
        let party_size = req.party_size.unwrap_or(1).clamp(1, MAX_PARTY);
        if party_size > max_players {
            return Err(ApiError::new(
                axum::http::StatusCode::BAD_REQUEST,
                "the party is larger than the requested match",
            ));
        }
        if !req.mmr.is_finite() {
            return Err(ApiError::new(
                axum::http::StatusCode::BAD_REQUEST,
                "mmr must be finite",
            ));
        }
        let ttl = req
            .ttl_ms
            .unwrap_or(DEFAULT_TTL_MS)
            .clamp(1_000, MAX_TTL_MS);

        let mut inner = self.inner.lock().map_err(lock_error)?;
        inner.expire(now);

        let id = Uuid::new_v4().simple().to_string();
        let ticket = Ticket {
            id: id.clone(),
            room,
            mode,
            region,
            mmr: req.mmr,
            party_size,
            max_players,
            created_ms: now,
            expires_ms: now.saturating_add(ttl),
            status: Status::Queued,
            seats: None,
        };
        inner.tickets.insert(id.clone(), ticket);
        inner.form_matches(now);
        let ticket = inner.tickets.get(&id).ok_or_else(|| {
            ApiError::new(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "ticket vanished",
            )
        })?;
        Ok(inner.view(ticket, now))
    }

    pub fn status(&self, ticket_id: &str) -> Result<QueueTicketView, ApiError> {
        self.status_at(ticket_id, now_ms())
    }

    fn status_at(&self, ticket_id: &str, now: u64) -> Result<QueueTicketView, ApiError> {
        let mut inner = self.inner.lock().map_err(lock_error)?;
        inner.expire(now);
        let ticket = inner.tickets.get(ticket_id).ok_or_else(not_found)?;
        Ok(inner.view(ticket, now))
    }

    pub fn leave(&self, ticket_id: &str) -> Result<QueueTicketView, ApiError> {
        self.leave_at(ticket_id, now_ms())
    }

    fn leave_at(&self, ticket_id: &str, now: u64) -> Result<QueueTicketView, ApiError> {
        let mut inner = self.inner.lock().map_err(lock_error)?;
        {
            let ticket = inner.tickets.get_mut(ticket_id).ok_or_else(not_found)?;
            if ticket.status == Status::Queued {
                ticket.status = Status::Cancelled;
            }
        }
        let ticket = inner.tickets.get(ticket_id).ok_or_else(not_found)?;
        Ok(inner.view(ticket, now))
    }

    /// Mint the handoff seats exactly once. The queue lock is held while the
    /// seat registry is called, so two status readers cannot mint two sets.
    pub fn seats(&self, ticket_id: &str) -> Result<Vec<QueueSeatView>, ApiError> {
        let mut inner = self.inner.lock().map_err(lock_error)?;
        let ticket = inner.tickets.get_mut(ticket_id).ok_or_else(not_found)?;
        let Status::Matched { room, .. } = &ticket.status else {
            return Err(ApiError::new(
                axum::http::StatusCode::CONFLICT,
                "the ticket is not matched yet",
            ));
        };
        if ticket.seats.is_none() {
            let mut seats = Vec::with_capacity(ticket.party_ids.len());
            for party_id in &ticket.party_ids {
                let desired = format!("{room}:{party_id}");
                let seat = {
                    let mut cur = self.plane.seats.load();
                    loop {
                        let mut next = (**cur).clone();
                        let minted = next.mint(&desired);
                        let prev = self.plane.seats.compare_and_swap(&*cur, Arc::new(next));
                        if std::sync::Arc::ptr_eq(&*cur, &*prev) {
                            break minted;
                        }
                        cur = prev;
                    }
                }
                .ok_or_else(|| {
                    ApiError::new(
                        axum::http::StatusCode::SERVICE_UNAVAILABLE,
                        "the server is at its outstanding-seat limit",
                    )
                })?;
                seats.push(seat);
            }
            ticket.seats = Some(seats);
        }
        Ok(ticket
            .seats
            .as_ref()
            .map(|seats| seats.iter().map(QueueSeatView::from_seat).collect())
            .unwrap_or_default())
    }

    /// Test-only deterministic clock entry point.
    #[cfg(test)]
    fn join_test(&self, req: QueueJoin, now: u64) -> Result<QueueTicketView, ApiError> {
        self.join_at(req, now)
    }

    #[cfg(test)]
    fn status_test(&self, ticket_id: &str, now: u64) -> Result<QueueTicketView, ApiError> {
        self.status_at(ticket_id, now)
    }

    #[cfg(test)]
    fn leave_test(&self, ticket_id: &str, now: u64) -> Result<QueueTicketView, ApiError> {
        self.leave_at(ticket_id, now)
    }
}

impl Inner {
    fn expire(&mut self, now: u64) {
        for ticket in self.tickets.values_mut() {
            if ticket.status == Status::Queued && now >= ticket.expires_ms {
                ticket.status = Status::Expired;
            }
        }
        // Keep terminal rows for ten minutes so a late status/leave read is
        // still answered, then drop them. This also bounds matched tickets.
        self.tickets
            .retain(|_, ticket| now < ticket.expires_ms.saturating_add(600_000));
    }

    fn form_matches(&mut self, now: u64) {
        let mut keys: Vec<(String, String, String, usize)> = self
            .tickets
            .values()
            .filter(|t| t.status == Status::Queued)
            .map(|t| {
                (
                    t.room.clone(),
                    t.mode.clone(),
                    t.region.clone(),
                    t.max_players,
                )
            })
            .collect();
        keys.sort();
        keys.dedup();
        for (room, mode, region, max) in keys {
            loop {
                let mut pool: Vec<String> = self
                    .tickets
                    .values()
                    .filter(|t| {
                        t.status == Status::Queued
                            && t.room == room
                            && t.mode == mode
                            && t.region == region
                            && t.max_players == max
                    })
                    .map(|t| t.id.clone())
                    .collect();
                if pool.len() < 2 && max > 1 {
                    // A single multi-player party can fill the room on its own.
                    let solo = self
                        .tickets
                        .values()
                        .find(|t| pool.iter().any(|id| id == &t.id))
                        .map(|t| t.party_size)
                        .unwrap_or(0);
                    if solo != max {
                        break;
                    }
                }
                pool.sort_by_key(|id| {
                    self.tickets
                        .get(id)
                        .map(|t| (t.created_ms, t.id.clone()))
                        .unwrap_or_default()
                });
                let mut chosen: Vec<String> = Vec::new();
                let mut total = 0usize;
                let mut anchor = f64::NAN;
                for id in &pool {
                    let Some(ticket) = self.tickets.get(id) else {
                        continue;
                    };
                    if chosen.is_empty() {
                        anchor = ticket.mmr;
                    }
                    let window = mmr_window(now.saturating_sub(ticket.created_ms));
                    if (ticket.mmr - anchor).abs() > window {
                        continue;
                    }
                    if total + ticket.party_size > max {
                        continue;
                    }
                    chosen.push(id.clone());
                    total += ticket.party_size;
                    if total == max {
                        break;
                    }
                }
                if total != max || chosen.is_empty() {
                    break;
                }
                let match_id = Uuid::new_v4().simple().to_string();
                for id in chosen {
                    if let Some(ticket) = self.tickets.get_mut(&id) {
                        ticket.status = Status::Matched {
                            match_id: match_id.clone(),
                            room: room.clone(),
                        };
                    }
                }
            }
        }
    }

    fn view(&self, ticket: &Ticket, now: u64) -> QueueTicketView {
        let status = match &ticket.status {
            Status::Queued => "queued",
            Status::Matched { .. } => "matched",
            Status::Cancelled => "cancelled",
            Status::Expired => "expired",
        };
        let (match_id, matched_room, seats) = match &ticket.status {
            Status::Matched { match_id, room } => (
                Some(match_id.clone()),
                Some(room.clone()),
                ticket
                    .seats
                    .as_ref()
                    .map(|seats| seats.iter().map(QueueSeatView::from_seat).collect())
                    .unwrap_or_default(),
            ),
            _ => (None, None, Vec::new()),
        };
        let position = if ticket.status == Status::Queued {
            let mut older = self
                .tickets
                .values()
                .filter(|other| {
                    other.status == Status::Queued
                        && other.room == ticket.room
                        && other.mode == ticket.mode
                        && other.region == ticket.region
                        && (other.created_ms, &other.id) < (ticket.created_ms, &ticket.id)
                })
                .count();
            older += 1;
            older
        } else {
            0
        };
        let waited = now.saturating_sub(ticket.created_ms);
        QueueTicketView {
            ticket_id: ticket.id.clone(),
            status: status.to_owned(),
            room: matched_room.unwrap_or_else(|| ticket.room.clone()),
            mode: ticket.mode.clone(),
            region: ticket.region.clone(),
            party_size: ticket.party_size,
            max_players: ticket.max_players,
            position,
            waited_ms: waited,
            eta_ms: (ticket.status == Status::Queued).then(|| waited.saturating_add(5_000)),
            match_id,
            seats,
            expires_at_ms: ticket.expires_ms,
        }
    }
}

fn mmr_window(waited_ms: u64) -> f64 {
    (BASE_MMR_WINDOW + (waited_ms / MMR_WINDOW_STEP_MS) as f64 * MMR_WINDOW_STEP)
        .min(MAX_MMR_WINDOW)
}

fn clean_room(room: &str) -> Result<String, ApiError> {
    let room = room.trim();
    if room.is_empty()
        || room.len() > 64
        || !room
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(ApiError::new(
            axum::http::StatusCode::BAD_REQUEST,
            "room must be 1-64 letters, digits, '-' or '_'",
        ));
    }
    Ok(room.to_owned())
}

fn clean_label(value: &str, field: &str) -> Result<String, ApiError> {
    let value = value.trim();
    if value.len() > 64 {
        return Err(ApiError::new(
            axum::http::StatusCode::BAD_REQUEST,
            format!("{field} must be at most 64 bytes"),
        ));
    }
    Ok(value.to_owned())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn lock_error(_: std::sync::PoisonError<std::sync::MutexGuard<'_, Inner>>) -> ApiError {
    ApiError::new(
        axum::http::StatusCode::SERVICE_UNAVAILABLE,
        "queue unavailable",
    )
}

fn not_found() -> ApiError {
    ApiError::new(axum::http::StatusCode::NOT_FOUND, "unknown queue ticket")
}

#[cfg(test)]
mod tests;
