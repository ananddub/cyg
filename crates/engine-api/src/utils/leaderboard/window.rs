//! Which record window a board's reads and writes address: the epoch for a plain
//! board, the next reset for a scheduled one, the round's end for a tournament.

use chrono::{DateTime, Utc};

use super::cron::Schedule;
use super::deadlines::at;
use crate::repositories::models::Leaderboard;
use crate::error::V2Error;

pub fn epoch() -> DateTime<Utc> {
    DateTime::from_timestamp(0, 0).expect("the epoch is a valid timestamp")
}

fn is_tournament(board: &Leaderboard) -> bool {
    board.duration > 0
}

/// The board's parsed reset schedule. BaaS rejects a define whose schedule does
/// not parse, so a stored expression that fails here is reported the same way.
pub(crate) fn parse_schedule(board: &Leaderboard) -> Result<Option<Schedule>, V2Error> {
    let Some(expression) = board.reset_schedule.as_deref().filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    Schedule::parse(expression)
        .map(Some)
        .ok_or_else(|| bad_schedule())
}

pub(crate) fn bad_schedule() -> V2Error {
    V2Error::invalid_argument("leaderboard reset schedule is not a cron expression")
}

/// The schedule's next boundary; a board without a schedule has no window of
/// its own and stays in the epoch bucket forever.
fn next_reset(board: &Leaderboard) -> Result<Option<DateTime<Utc>>, V2Error> {
    let Some(schedule) = parse_schedule(board)? else {
        return Ok(None);
    };
    schedule
        .next_after(Utc::now())
        .map(Some)
        .ok_or_else(bad_schedule)
}

/// BaaS `LeaderboardRecordWrite`'s window: the next reset, or the epoch.
pub fn record_window(board: &Leaderboard) -> Result<DateTime<Utc>, V2Error> {
    Ok(next_reset(board)?.unwrap_or_else(epoch))
}

/// BaaS `calculateTournamentDeadlines`' expiry: when the round the caller is
/// in stops taking scores.
pub fn tournament_window(board: &Leaderboard) -> Result<DateTime<Utc>, V2Error> {
    Ok(at(
        super::deadlines::calculate(board, Utc::now())?.expiry,
    ))
}

/// BaaS `calculateExpiryOverride`. `None` means the window has already
/// closed, which is an empty page rather than an error.
pub fn query_window(
    board: &Leaderboard,
    override_expiry: i64,
) -> Result<Option<DateTime<Utc>>, V2Error> {
    if override_expiry != 0 {
        return Ok(DateTime::from_timestamp(override_expiry, 0));
    }
    if is_tournament(board) {
        let expiry = tournament_window(board)?;
        if expiry.timestamp() != 0 && expiry <= Utc::now() {
            return Ok(None);
        }
        return Ok(Some(expiry));
    }
    Ok(Some(record_window(board)?))
}
