//! BaaS `calculateTournamentDeadlines`: the three boundaries of the round the
//! caller stands in. A scheduled tournament repeats, so the round began at the
//! last reset at or before now (or the next one, if the board has not begun);
//! it lasts `duration` seconds and never runs past the next reset or `end_time`.

use chrono::{DateTime, Duration, Utc};

use super::cron::Schedule;
use super::window::{bad_schedule, epoch, parse_schedule};
use crate::repositories::models::Leaderboard;
use crate::error::V2Error;

/// Unix seconds are what the wire carries, so every boundary is one.
pub struct Deadlines {
    pub start_active: i64,
    pub end_active: i64,
    pub expiry: i64,
}

pub fn at(seconds: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(seconds, 0).unwrap_or_else(epoch)
}

fn next(schedule: &Schedule, from: DateTime<Utc>) -> Result<i64, V2Error> {
    schedule
        .next_after(from)
        .map(|t| t.timestamp())
        .ok_or_else(bad_schedule)
}

fn last(schedule: &Schedule, from: DateTime<Utc>) -> Result<i64, V2Error> {
    schedule
        .last_before(from)
        .map(|t| t.timestamp())
        .ok_or_else(bad_schedule)
}

pub fn calculate(board: &Leaderboard, now: DateTime<Utc>) -> Result<Deadlines, V2Error> {
    let start = board.start_time.timestamp();
    let end = board.end_time.timestamp();
    let duration = i64::from(board.duration);
    let now_unix = now.timestamp();

    let Some(schedule) = parse_schedule(board)? else {
        let mut end_active = start + duration;
        if end > 0 && end_active > end {
            end_active = end;
        }
        return Ok(Deadlines {
            start_active: start,
            end_active,
            expiry: end,
        });
    };

    let lands_on_schedule = schedule
        .next_after(now - Duration::seconds(1))
        .is_some_and(|n| n.timestamp() == now_unix);
    let mut start_active = if now_unix < start {
        next(&schedule, at(start))?
    } else if lands_on_schedule {
        now_unix
    } else {
        last(&schedule, now)?
    };

    let mut end_active = start_active + duration;
    let mut expiry = next(&schedule, at(start_active))?;
    if end_active > expiry {
        end_active = expiry;
    }

    if start > end_active {
        // The board started after the round we landed in but before the next
        // reset, so the caller is in the pair of boundaries the start begins.
        let pair = schedule
            .next_many(at(start), 2)
            .ok_or_else(bad_schedule)?;
        start_active = pair[0].timestamp();
        expiry = pair[1].timestamp();
        end_active = start_active + duration;
        if end_active > expiry {
            end_active = expiry;
        }
    } else if start > start_active {
        start_active = start;
    }

    if end > 0 && expiry > end {
        expiry = end;
        if end_active > expiry {
            end_active = expiry;
        }
    }

    Ok(Deadlines {
        start_active,
        end_active,
        expiry,
    })
}

/// BaaS `calculatePrevReset`: the boundary the caller has just passed.
pub fn prev_reset(board: &Leaderboard, now: DateTime<Utc>) -> Result<i64, V2Error> {
    let Some(schedule) = parse_schedule(board)? else {
        return Ok(0);
    };
    if at(board.start_time.timestamp()) > now {
        return Ok(0);
    }
    last(&schedule, now)
}
