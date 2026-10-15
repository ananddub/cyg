use std::net::IpAddr;

use axum::http::StatusCode;

use super::routes::{MATCHMAKE_METHODS, valid_room_name};
use super::wire::SeatResponse;
use crate::ApiError;
use crate::ApiState;
use crate::seats::SeatRegistry;

/// The endpoint, as a function of the socket's source address.
///
/// `pub` so the rate-limit-before-crypto property can be tested directly: a
/// loopback client can only ever present one IP, so the multi-source rule needs
/// the address handed in.
pub fn reserve(
    state: &ApiState,
    ip: IpAddr,
    method: &str,
    room: &str,
    body: &str,
) -> Result<SeatResponse, ApiError> {
    // 1. Charge this IP *before* anything expensive runs. A poisoned lock fails
    //    open: the limiter is defence in depth, the seat signature is the gate.
    let charged = state.limiter.lock().map(|mut l| l.allow(ip)).unwrap_or(true);
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

    // 3. `reconnect` and `rejoin` consume an existing token instead of minting:
    //    same `sessionId`, same room, so the WebSocket upgrade query and the
    //    raw-UDP `JOIN` both survive a reload.
    if method == "reconnect" || method == "rejoin" {
        return resume_seat(state, body);
    }

    // 4. The rest all mint: the room host owns membership, this plane only
    //    signs the seat.
    let seat = state
        .seats
        .lock()
        .map(|mut r: std::sync::MutexGuard<'_, SeatRegistry>| {
            r.mint(room, state.control.seat_ttl_secs)
        })
        .map_err(|_| ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "seat registry unavailable"))?
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "the server is at its outstanding-seat limit",
            )
        })?;

    Ok(SeatResponse::new(state, &seat))
}

/// The client re-POSTs `reconnect` with its `reconnectionToken`; the seat it
/// belongs to is handed back unchanged, so room membership survives a reload.
fn resume_seat(state: &ApiState, body: &str) -> Result<SeatResponse, ApiError> {
    let token = super::wire::json_string_field(body, "reconnectionToken")
        .ok_or_else(|| ApiError::new(StatusCode::BAD_REQUEST, "missing reconnectionToken"))?;
    let seat = state
        .seats
        .lock()
        .map(|mut r| r.verify(&token))
        .map_err(|_| ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "seat registry unavailable"))?
        .ok_or_else(|| {
            ApiError::new(StatusCode::UNAUTHORIZED, "reconnection token expired or unknown")
        })?;
    Ok(SeatResponse::new(state, &seat))
}

/// Error text goes into a JSON string, and `ApiError` quotes with Rust's
/// `Debug`, which renders a control character as `\u{1}` — not valid JSON.
/// The method name arrives percent-decoded from a URL, so those bytes are
/// dropped here rather than echoed.
pub(super) fn printable(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_graphic() || *c == ' ')
        .take(64)
        .collect()
}
