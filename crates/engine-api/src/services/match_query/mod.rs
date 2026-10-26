//! The read side of the match and matchmaker REST surface.
//!
//! This plane keeps matches and queued tickets in process registries, not in a
//! database, so the logic here is only validation and shaping: read the registry
//! through `services::match_handler` / `services::matchmaker` and print the reference
//! server wire shapes. No query runs, no table is touched.

use auto_di::singleton;
use axum::Json;

#[cfg(test)]
mod tests;

use crate::dto::matches::{
    ListMatchesParams, MatchListResponse, MatchResponse, MatchmakerStatsResponse, MatchsetStat,
};
use crate::error::V2Error;
use crate::services::realtime::{RealtimeMatchService, RealtimeMatchmakerService};

/// The default page size, and the ceiling, both the reference server's.
const DEFAULT_LIMIT: i64 = 10;
const MAX_LIMIT: i64 = 100;

pub struct MatchQueryService;

#[singleton]
impl MatchQueryService {
    fn new() -> Self {
        Self
    }

    /// `GET /v2/match`. Validates first, exactly as the reference server does, then reads the
    /// registry and applies the filters it can honour. The `query` label filter is
    /// dropped: the reference server itself rejects label/query filtering for non-authoritative
    /// matches, and every match on this plane is one.
    pub fn list_matches(&self, req: ListMatchesParams) -> Result<Json<MatchListResponse>, V2Error> {
        let limit = match req.limit {
            None => DEFAULT_LIMIT,
            Some(value) if !(1..=MAX_LIMIT).contains(&value) => {
                return Err(V2Error::invalid_argument(
                    "Invalid limit - limit must be between 1 and 100.",
                ));
            }
            Some(value) => value,
        };
        if req.min_size.is_some_and(|value| value < 0) {
            return Err(V2Error::invalid_argument(
                "Minimum size must be 0 or above.",
            ));
        }
        if req.max_size.is_some_and(|value| value < 0) {
            return Err(V2Error::invalid_argument(
                "Maximum size must be 0 or above.",
            ));
        }
        if let (Some(min), Some(max)) = (req.min_size, req.max_size) {
            if min > max {
                return Err(V2Error::invalid_argument(
                    "Maximum size must be greater than or equal to minimum size when both are specified.",
                ));
            }
        }

        let mut matches: Vec<MatchResponse> = RealtimeMatchService::list_snapshots_all()
            .iter()
            .map(MatchResponse::from)
            .collect();
        if let Some(authoritative) = req.authoritative {
            matches.retain(|m| m.authoritative == authoritative);
        }
        if let Some(label) = req.label.as_deref().filter(|l| !l.is_empty()) {
            matches.retain(|m| m.label.as_deref() == Some(label));
        }
        if let Some(min) = req.min_size {
            matches.retain(|m| i64::from(m.size) >= min);
        }
        if let Some(max) = req.max_size {
            matches.retain(|m| i64::from(m.size) <= max);
        }
        matches.sort_by(|a, b| a.match_id.cmp(&b.match_id));
        matches.truncate(limit as usize);
        Ok(Json(MatchListResponse { matches }))
    }

    /// `GET /v2/matchmaker/stats`. Reports the real queue; `completions` stays empty
    /// because finished-ticket history is not kept here.
    pub fn matchmaker_stats(&self) -> Json<MatchmakerStatsResponse> {
        let snapshot = RealtimeMatchmakerService::get_stats();
        let matchsets: Vec<MatchsetStat> = snapshot
            .matchsets
            .into_iter()
            .map(|s| MatchsetStat {
                matchset: s.matchset,
                ticket_count: s.ticket_count,
                oldest_create_time: s.oldest_create_time,
                avg_wait_secs: (s.avg_wait_secs * 100.0).round() / 100.0,
            })
            .collect();
        Json(MatchmakerStatsResponse {
            ticket_count: snapshot.ticket_count,
            oldest_ticket_create_time: snapshot.oldest_ticket_create_time,
            completions: Vec::new(),
            matchsets,
        })
    }
}
