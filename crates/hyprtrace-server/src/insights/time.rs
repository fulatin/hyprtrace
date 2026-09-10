use chrono::{Datelike, Duration as ChronoDuration, NaiveDate};

/// Today's local calendar date as `YYYY-MM-DD`.
pub fn today_local() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// Default `from` for endpoints that take an inclusive range: 6 days ago, so a
/// bare request covers the last 7 local days.
pub fn default_from() -> String {
    (chrono::Local::now() - ChronoDuration::days(6))
        .format("%Y-%m-%d")
        .to_string()
}

pub(crate) fn date_shift(date: &str, days: i64) -> Option<String> {
    NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .ok()
        .map(|d| (d + ChronoDuration::days(days)).format("%Y-%m-%d").to_string())
}

pub(crate) fn parse_date(date: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()
}

/// Weekday index with Monday = 0 .. Sunday = 6.
pub(crate) fn weekday_mon0(date: NaiveDate) -> u8 {
    date.weekday().num_days_from_monday() as u8
}
