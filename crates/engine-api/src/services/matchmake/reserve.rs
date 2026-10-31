use std::net::IpAddr;

use axum::http::StatusCode;

use super::MatchmakeService;
use crate::ApiError;
use crate::ApiState;
use crate::dto::matchmake::{SeatResponse, json_string_field};
use crate::infra::seats::SeatRegistry;
use crate::utils::matchmake::{MATCHMAKE_METHODS, printable, valid_room_name};

impl MatchmakeService {
    pub fn reserve(
        &self,
        state: &ApiState,
        ip: IpAddr,
        method: &str,
        room: &str,
        body: &str,
    ) -> Result<SeatResponse, ApiError> {
        let charged = state
            .limiter
            .lock()
            .map(|mut l| l.allow(ip))
            .unwrap_or(true);
        if !charged {
            let wait = state.limiter.lock().map(|l| l.retry_after(ip)).unwrap_or(1);
            return Err(ApiError::rate_limited(wait));
        }

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

        if method == "reconnect" || method == "rejoin" {
            return resume_seat(state, body);
        }

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

pub fn reserve(
    state: &ApiState,
    ip: IpAddr,
    method: &str,
    room: &str,
    body: &str,
) -> Result<SeatResponse, ApiError> {
    MatchmakeService.reserve(state, ip, method, room, body)
}

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
