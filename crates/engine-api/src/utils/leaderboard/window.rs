use chrono::{DateTime, Utc};

use super::cron::Schedule;
use super::deadlines::at;
use crate::error::V2Error;
use crate::repositories::models::Leaderboard;

pub fn epoch() -> DateTime<Utc> {
    DateTime::from_timestamp(0, 0).expect("the epoch is a valid timestamp")
}

fn is_tournament(board: &Leaderboard) -> bool {
    board.duration > 0
}

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

fn next_reset(board: &Leaderboard) -> Result<Option<DateTime<Utc>>, V2Error> {
    let Some(schedule) = parse_schedule(board)? else {
        return Ok(None);
    };
    schedule
        .next_after(Utc::now())
        .map(Some)
        .ok_or_else(bad_schedule)
}

pub fn record_window(board: &Leaderboard) -> Result<DateTime<Utc>, V2Error> {
    Ok(next_reset(board)?.unwrap_or_else(epoch))
}

pub fn tournament_window(board: &Leaderboard) -> Result<DateTime<Utc>, V2Error> {
    Ok(at(super::deadlines::calculate(board, Utc::now())?.expiry))
}

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
