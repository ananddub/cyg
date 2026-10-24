//! `POST /api/matchmake/{method}/{room}` (and `/matchmake/…`) — mint or resume a seat.
//!
//! The order is the point: bucket, then method/room, then HMAC, then map write. A seat's room
//! comes from the *signature*, never the URL.

use std::net::IpAddr;

use axum::http::StatusCode;

use super::MatchmakeService;
use crate::ApiError;
use crate::ApiState;
use crate::dto::matchmake::{SeatResponse, json_string_field};
use crate::infra::seats::SeatRegistry;
use crate::utils::matchmake::{MATCHMAKE_METHODS, printable, valid_room_name};

impl MatchmakeService {
    /// The endpoint, as a function of the socket's source address.
    pub fn reserve(
        &self,
        state: &ApiState,
        ip: IpAddr,
        method: &str,
        room: &str,
        body: &str,
    ) -> Result<SeatResponse, ApiError> {
        // 1. Charge this IP *before* anything expensive runs. A poisoned lock fails
        //    open: the limiter is defence in depth, the seat signature is the gate.
        let charged = state
            .limiter
            .lock()
            .map(|mut l| l.allow(ip))
            .unwrap_or(true);
        if !charged {
            // `Retry-After` comes out of the same bucket that just said no, so a
            // page that waits the advertised time is not refused again.
            let wait = state.limiter.lock().map(|l| l.retry_after(ip)).unwrap_or(1);
            return Err(ApiError::rate_limited(wait));
        }

        // 2. Cheap validation, still before any MAC work.
        if !MATCHMAKE_METHODS.contains(&method) {
            return Err(ApiError::new(
                StatusCode::NOT_FOUND,
                format!("no room handler for method '{}'", printable(method)),
            ));
        }
        if !valid_room_name(room) {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "invalid room name: use 1-64 letters, digits, '-' or '_'",
            ));
        }

        // 3. `reconnect`/`rejoin` consume an existing token instead of minting: same
        //    `sessionId`, same room, so a reload survives.
        if method == "reconnect" || method == "rejoin" {
            return resume_seat(state, body);
        }

        // 4. The rest mint: the room host owns membership, this plane only signs.
        let seat = state
            .seats
            .lock()
            .map(|mut r: std::sync::MutexGuard<'_, SeatRegistry>| {
                r.mint(room, state.control.seat_ttl_secs)
            })
            .map_err(|_| {
                ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "seat registry unavailable")
            })?
            .ok_or_else(|| {
                ApiError::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "the server is at its outstanding-seat limit",
                )
            })?;

        Ok(SeatResponse::new(state, &seat))
    }
}

/// The endpoint as a free function: the spelling the seat tests drive directly, so
/// the rate-limit-before-crypto property is testable without HTTP.
pub fn reserve(
    state: &ApiState,
    ip: IpAddr,
    method: &str,
    room: &str,
    body: &str,
) -> Result<SeatResponse, ApiError> {
    MatchmakeService.reserve(state, ip, method, room, body)
}

/// The client re-POSTs `reconnect` with its `reconnectionToken`; the seat it
/// belongs to is handed back unchanged, so room membership survives a reload.
fn resume_seat(state: &ApiState, body: &str) -> Result<SeatResponse, ApiError> {
    let token = json_string_field(body, "reconnectionToken")
        .ok_or_else(|| ApiError::new(StatusCode::BAD_REQUEST, "missing reconnectionToken"))?;
    let seat = state
        .seats
        .lock()
        .map(|mut r| r.verify(&token))
        .map_err(|_| ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "seat registry unavailable"))?
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::UNAUTHORIZED,
                "reconnection token expired or unknown",
            )
        })?;
    Ok(SeatResponse::new(state, &seat))
}
