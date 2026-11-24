use std::sync::Arc;

use crate::services::plane::Plane;

use std::net::IpAddr;

use axum::http::StatusCode;

use super::MatchmakeService;
use crate::ApiError;
use crate::controllers::web::routes::mm::{
    JoinByIdRequest, MatchmakeTokenResponse as SeatResponse, ReserveRequest,
};
use crate::infra::seats::MAX_TTL_DRIFT_SECS;

impl MatchmakeService {
    /// `POST /matchmake/:method/:room` — the primary seat endpoint.
    #[allow(non_snake_case)]
    pub async fn reserve(
        &self,
        method: String,
        room: String,
        clientIp: IpAddr,
        body: ReserveRequest,
    ) -> Result<SeatResponse, ApiError> {
        let method = method.to_ascii_lowercase();
        let valid_method = matches!(method.as_str(), "join" | "joinorcreate" | "create" | "joinbyid" | "reconnect");
        if !valid_method {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                format!("unknown matchmake method '{}'", printable(&method)),
            ));
        }
        // Rate-limit: per-IP sharded check.
        if !self.plane.limiter.check_and_count(clientIp) {
            return Err(ApiError::new(
                StatusCode::TOO_MANY_REQUESTS,
                "rate limit exceeded — try again in a moment",
            ));
        }

        let room_key = self.resolve_room(&method, &room, &body).await?;
        self.mint_seat(&room_key, &body).await
    }

    /// `POST /matchmake/joinById` — body-only form of joinById.
    #[allow(non_snake_case)]
    pub async fn reserve_join_by_id(
        &self,
        clientIp: IpAddr,
        body: JoinByIdRequest,
    ) -> Result<SeatResponse, ApiError> {
        if !self.plane.limiter.check_and_count(clientIp) {
            return Err(ApiError::new(
                StatusCode::TOO_MANY_REQUESTS,
                "rate limit exceeded — try again in a moment",
            ));
        }
        let room = body.roomId.clone();
        let reserve_body = ReserveRequest::default();
        let room_key = self.resolve_room("joinbyid", &room, &reserve_body).await?;
        self.mint_seat(&room_key, &reserve_body).await
    }

    async fn resolve_room(
        &self,
        method: &str,
        room: &str,
        body: &ReserveRequest,
    ) -> Result<String, ApiError> {
        // Reconnect: the body carries the short reconnection token that the
        // host wrote at onLeave. We only hand out a fresh seat when the token
        // is still inside the grace window.
        if method == "reconnect" {
            let token = body.reconnectionToken.as_deref().ok_or_else(|| {
                ApiError::new(StatusCode::BAD_REQUEST, "reconnectionToken is required")
            })?;
            let seat = self
                .plane
                .sessions
                .take_reconnection(token)
                .ok_or_else(|| {
                    ApiError::new(
                        StatusCode::UNAUTHORIZED,
                        "reconnection token expired or unknown",
                    )
                })?;
            return Ok(seat.room);
        }

        if method == "joinById" {
            // joinById: resolve an exact existing room by roomId or name.
            // Fall back to the room name when no status snapshot is available,
            // so joinById still works in single-node/dev flows before status publishes.
            let snapshot = self.plane.latest_status.load_full();
            let found = snapshot.as_deref().and_then(|s| {
                s.rooms
                    .iter()
                    .find(|r| r.room_id == room || r.name == room)
                    .map(|r| r.name.clone())
            });
            if let Some(name) = found {
                return Ok(name);
            }
            if snapshot.is_some() {
                return Err(ApiError::new(
                    StatusCode::NOT_FOUND,
                    format!("room '{}' not found", printable(room)),
                ));
            }
            return Ok(room.to_string());
        }

        match self.plane.game.rooms.get(room) {
            Some(variant) => {
                let base = format!("{room}{}", variant.room_key_suffix(body));
                let cap = variant
                    .max_clients
                    .unwrap_or(self.plane.game.max_clients);
                let now = std::time::Instant::now();
                let snapshot = self.plane.latest_status.load_full();
                let occupancy = |key: &str| -> u32 {
                    snapshot
                        .as_deref()
                        .map(|s| {
                            s.rooms
                                .iter()
                                .find(|r| r.name == key)
                                .map(|r| r.clients)
                                .unwrap_or(0)
                        })
                        .unwrap_or(0)
                };

                let generation = match method {
                    "create" => {
                        let mut cur = self.plane.chain.load();
                        loop {
                            let mut next = (**cur).clone();
                            let g = next.pick_create_generation(&base, cap, &occupancy, now);
                            let prev = self.plane.chain.compare_and_swap(&*cur, Arc::new(next));
                            if Arc::ptr_eq(&*cur, &*prev) {
                                break g;
                            }
                            cur = prev;
                        }
                    }
                    "join" => {
                        let mut cur = self.plane.chain.load();
                        let g = loop {
                            let mut next = (**cur).clone();
                            let g = next.pick_join_generation(&base, cap, &occupancy, now);
                            let prev = self.plane.chain.compare_and_swap(&*cur, Arc::new(next));
                            if Arc::ptr_eq(&*cur, &*prev) {
                                break g;
                            }
                            cur = prev;
                        };
                        g.ok_or_else(|| {
                            ApiError::new(
                                StatusCode::NOT_FOUND,
                                format!("no available room '{}' to join", printable(room)),
                            )
                        })?
                    }
                    _ => {
                        let mut cur = self.plane.chain.load();
                        loop {
                            let mut next = (**cur).clone();
                            let g = next.pick_generation(&base, cap, &occupancy, now);
                            let prev = self.plane.chain.compare_and_swap(&*cur, Arc::new(next));
                            if Arc::ptr_eq(&*cur, &*prev) {
                                break g;
                            }
                            cur = prev;
                        }
                    }
                };
                let key = crate::services::matchmake::chain::generation_key(&base, generation);
                Ok(key)
            }
            None => {
                if method == "create" || method == "joinorcreate" {
                    Ok(room.to_string())
                } else {
                    // join: the room must exist on the host already.
                    let exists = snapshot_room_exists(self, room);
                    if !exists {
                        return Err(ApiError::new(
                            StatusCode::NOT_FOUND,
                            format!("room '{}' not found", printable(room)),
                        ));
                    }
                    Ok(room.to_string())
                }
            }
        }
    }

    async fn mint_seat(
        &self,
        room_key: &str,
        body: &ReserveRequest,
    ) -> Result<SeatResponse, ApiError> {
        let seat = {
            let mut cur = self.plane.seats.load();
            loop {
                let mut next = (**cur).clone();
                let minted =
                    next.mint_with_options(room_key, self.plane.control.seat_ttl_secs, body);
                let prev = self.plane.seats.compare_and_swap(&*cur, Arc::new(next));
                if Arc::ptr_eq(&*cur, &*prev) {
                    break minted;
                }
                cur = prev;
            }
        }
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "the server is at its outstanding-seat limit",
            )
        })?;

        // Admission hook: the game host decides if this seat is welcome.
        if let Some(hook) = &self.plane.hooks {
            if let Err(e) = hook.on_reserve(room_key, &seat).await {
                return Err(ApiError::new(StatusCode::FORBIDDEN, e));
            }
        }

        Ok(SeatResponse::new(&self.plane, &seat))
    }
}

fn snapshot_room_exists(svc: &MatchmakeService, room: &str) -> bool {
    let snapshot = svc.plane.latest_status.load_full();
    snapshot
        .as_deref()
        .map(|s| s.rooms.iter().any(|r| r.name == room || r.room_id == room))
        .unwrap_or(false)
}

/// Replace unprintable chars so a user-supplied name can't smuggle control
/// sequences into an error message that ends up in logs or a UI.
fn printable(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_control() {
                char::REPLACEMENT_CHARACTER
            } else {
                c
            }
        })
        .collect()
}
