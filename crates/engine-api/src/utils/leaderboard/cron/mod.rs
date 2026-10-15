//! Reset schedules. A leaderboard's `reset_schedule` is a cron expression, and a
//! record's window is the schedule's next occurrence, so the board must answer
//! "when does this reset next?" and "when did it last?". The standard 5-field
//! subset, evaluated in UTC at minute granularity; anything else is refused.

mod days;
mod fields;

use chrono::{DateTime, Datelike, NaiveDate, Timelike, Utc};
use days::{end_of_prev_month, next_day, on_day_at, prev_day, start_of_next_month, trim_minute};
use fields::{MONTHS, WEEKDAYS, field};

/// Four years of days is enough for any 5-field expression.
const FOUR_YEARS: i64 = 366 * 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Schedule {
    minute: [bool; 60],
    hour: [bool; 24],
    day: [bool; 32],
    month: [bool; 13],
    weekday: [bool; 7],
    day_star: bool,
    weekday_star: bool,
}

impl Schedule {
    pub fn parse(expression: &str) -> Option<Schedule> {
        let fields: Vec<&str> = expression.split_whitespace().collect();
        if fields.len() != 5 {
            return None;
        }
        let (minute, _) = field(fields[0], 0, 59, &[])?;
        let (hour, _) = field(fields[1], 0, 23, &[])?;
        let (day, day_star) = field(fields[2], 1, 31, &[])?;
        let (month, _) = field(fields[3], 1, 12, &MONTHS)?;
        let (weekday, weekday_star) = field(fields[4], 0, 6, &WEEKDAYS)?;
        Some(Schedule {
            minute: minute.try_into().ok()?,
            hour: hour.try_into().ok()?,
            day: day.try_into().ok()?,
            month: month.try_into().ok()?,
            weekday: weekday.try_into().ok()?,
            day_star,
            weekday_star,
        })
    }

    /// The first occurrence strictly after `after`.
    pub fn next_after(&self, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
        let limit = after + chrono::Duration::days(FOUR_YEARS);
        let mut candidate = trim_minute(after) + chrono::Duration::minutes(1);
        while candidate <= limit {
            let day = candidate.date_naive();
            if !self.month[day.month() as usize] {
                candidate = start_of_next_month(day);
                continue;
            }
            if !self.on_day(day) {
                candidate = on_day_at(next_day(day), 0, 0);
                continue;
            }
            if !self.hour[candidate.hour() as usize] {
                candidate = if candidate.hour() == 23 {
                    on_day_at(next_day(day), 0, 0)
                } else {
                    on_day_at(day, candidate.hour() + 1, 0)
                };
                continue;
            }
            if !self.minute[candidate.minute() as usize] {
                candidate += chrono::Duration::minutes(1);
                continue;
            }
            return Some(candidate);
        }
        None
    }

    /// The last occurrence strictly before `before`, walking the same field tests backwards.
    pub fn last_before(&self, before: DateTime<Utc>) -> Option<DateTime<Utc>> {
        let limit = before - chrono::Duration::days(FOUR_YEARS);
        let mut candidate = trim_minute(before) - chrono::Duration::minutes(1);
        while candidate >= limit {
            let day = candidate.date_naive();
            if !self.month[day.month() as usize] {
                candidate = end_of_prev_month(day);
                continue;
            }
            if !self.on_day(day) {
                candidate = on_day_at(prev_day(day), 23, 59);
                continue;
            }
            if !self.hour[candidate.hour() as usize] {
                candidate = if candidate.hour() == 0 {
                    on_day_at(prev_day(day), 23, 59)
                } else {
                    on_day_at(day, candidate.hour() - 1, 59)
                };
                continue;
            }
            if !self.minute[candidate.minute() as usize] {
                candidate -= chrono::Duration::minutes(1);
                continue;
            }
            return Some(candidate);
        }
        None
    }

    /// `count` consecutive occurrences after `after`, oldest first.
    pub fn next_many(&self, after: DateTime<Utc>, count: usize) -> Option<Vec<DateTime<Utc>>> {
        let mut out = Vec::with_capacity(count);
        let mut cursor = after;
        for _ in 0..count {
            let next = self.next_after(cursor)?;
            out.push(next);
            cursor = next;
        }
        Some(out)
    }

    /// Traditional cron: a restricted day-of-week and a restricted day-of-month
    /// each get to admit the day on their own.
    fn on_day(&self, day: NaiveDate) -> bool {
        let by_month_day = self.day[day.day() as usize];
        let by_weekday = self.weekday[day.weekday().num_days_from_sunday() as usize];
        match (self.day_star, self.weekday_star) {
            (true, true) => true,
            (true, false) => by_weekday,
            (false, true) => by_month_day,
            (false, false) => by_month_day || by_weekday,
        }
    }
}
