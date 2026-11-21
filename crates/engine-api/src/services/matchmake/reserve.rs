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
        let charged = state.limiter.allow(ip);
        if !charged {
            let wait = state.limiter.retry_after(ip).max(1);
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

        // Room variants ([rooms.<name>] in config): the variant's filter_by
        // keys split the join options into a deterministic room key, so
        // `joinOrCreate("duo", { region: "mumbai" })` lands in the duo
        // room for mumbai — and equal filters land in the SAME room.
        // When that room reaches the variant's cap, the join chains into the
        // next generation of the same variant (`...__g2`) instead of squeezing
        // in or dying with "room is full".
        let (room_key, _variant) = match state.game.rooms.get(room) {
            Some(variant) => {
                let base = format!("{room}{}", variant.room_key_suffix(body));
                let cap = variant.max_clients.unwrap_or(state.game.max_clients);
                let now = std::time::Instant::now();
                let generation = {
                    let mut chain = state.chain.lock().map_err(|_| {
                        ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "seat registry unavailable")
                    })?;
                    let snapshot = state.latest_status.lock().ok();
                    let occupancy = |key: &str| -> u32 {
                        snapshot
                            .as_deref()
                            .and_then(|snapshot| snapshot.as_ref())
                            .map(|snapshot| {
                                snapshot
                                    .rooms
                                    .iter()
                                    .filter(|room| room.name == key)
                                    .map(|room| room.clients)
                                    .sum()
                            })
                            .unwrap_or(0)
                    };
                    // Pick and record inside one lock acquisition: recording
                    // the fresh seat before releasing the tracker is what makes
                    // the next concurrent join see it, so a split lock window
                    // would let a burst all land in the same full room.
                    let generation = chain.pick_generation(&base, cap, occupancy, now);
                    let key = if generation == 0 {
                        base.clone()
                    } else {
                        format!("{base}__g{generation}")
                    };
                    chain.record(&key, now);
                    generation
                };
                let key = if generation == 0 {
                    base.clone()
                } else {
                    format!("{base}__g{generation}")
                };
                (key, Some(variant.clone()))
            }
            None => (room.to_string(), None),
        };

        let seat = state
            .seats
            .lock()
            .map(|mut r: std::sync::MutexGuard<'_, SeatRegistry>| {
                r.mint_with_options(&room_key, state.control.seat_ttl_secs, body)
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
