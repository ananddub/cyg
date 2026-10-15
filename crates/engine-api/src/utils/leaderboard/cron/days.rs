//! Calendar arithmetic the scheduler walks with, in UTC and in both directions.

use chrono::{DateTime, Datelike, NaiveDate, NaiveTime, TimeZone, Timelike, Utc};

pub fn trim_minute(t: DateTime<Utc>) -> DateTime<Utc> {
    on_day_at(t.date_naive(), t.hour(), t.minute())
}

pub fn on_day_at(day: NaiveDate, hour: u32, minute: u32) -> DateTime<Utc> {
    let time = NaiveTime::from_hms_opt(hour, minute, 0).expect("hour and minute are in range");
    Utc.from_utc_datetime(&day.and_time(time))
}

pub fn next_day(day: NaiveDate) -> NaiveDate {
    day + chrono::Duration::days(1)
}

pub fn prev_day(day: NaiveDate) -> NaiveDate {
    day - chrono::Duration::days(1)
}

/// The last minute of the month before `day`, so a backwards scan that fails
/// the month test lands on a candidate still inside it.
pub fn end_of_prev_month(day: NaiveDate) -> DateTime<Utc> {
    let (year, month) = if day.month() == 1 {
        (day.year() - 1, 12)
    } else {
        (day.year(), day.month() - 1)
    };
    on_day_at(last_day_of_month(year, month), 23, 59)
}

pub fn start_of_next_month(day: NaiveDate) -> DateTime<Utc> {
    let (year, month) = if day.month() == 12 {
        (day.year() + 1, 1)
    } else {
        (day.year(), day.month() + 1)
    };
    let first = NaiveDate::from_ymd_opt(year, month, 1).expect("the 1st of a month exists");
    Utc.from_utc_datetime(&first.and_time(chrono::NaiveTime::MIN))
}

fn last_day_of_month(year: i32, month: u32) -> NaiveDate {
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let first = NaiveDate::from_ymd_opt(next_year, next_month, 1).expect("the 1st of a month exists");
    first - chrono::Duration::days(1)
}
