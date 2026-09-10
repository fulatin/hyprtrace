use chrono::Duration as ChronoDuration;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

use super::time::{parse_date, weekday_mon0};

// ---------------------------------------------------------------------------
// 3. Rhythm
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct RhythmCell {
    pub weekday: u8,
    pub hour: u8,
    pub total_ms: i64,
    pub avg_ms: i64,
    pub days: i64,
}

#[derive(Debug, Serialize)]
pub struct RhythmResponse {
    #[serde(rename = "from")]
    pub from: String,
    #[serde(rename = "to")]
    pub to: String,
    pub max_avg_ms: i64,
    pub cells: Vec<RhythmCell>,
}

/// Average weekday x hour usage. `avg_ms` divides the cell total by the number
/// of distinct dates in the range that fall on that weekday, so a weekday that
/// only occurs twice is not diluted by the other five days.
pub fn rhythm(
    rows: &[(String, u8, i64)],
    from: &str,
    to: &str,
) -> RhythmResponse {
    let mut weekday_dates: [HashSet<String>; 7] = Default::default();
    if let (Some(a), Some(b)) = (parse_date(from), parse_date(to)) {
        let mut cur = a;
        while cur <= b {
            weekday_dates[weekday_mon0(cur) as usize].insert(cur.format("%Y-%m-%d").to_string());
            cur += ChronoDuration::days(1);
        }
    }

    let mut totals: HashMap<(u8, u8), i64> = HashMap::new();
    for (date, hour, total) in rows {
        let Some(d) = parse_date(date) else { continue };
        if *hour > 23 {
            continue;
        }
        *totals.entry((weekday_mon0(d), *hour)).or_insert(0) += total;
    }

    let mut cells: Vec<RhythmCell> = totals
        .into_iter()
        .filter(|(_, total)| *total > 0)
        .map(|((weekday, hour), total)| {
            let days = weekday_dates[weekday as usize].len() as i64;
            RhythmCell {
                weekday,
                hour,
                total_ms: total,
                avg_ms: if days > 0 { total / days } else { 0 },
                days,
            }
        })
        .collect();
    cells.sort_by(|a, b| a.weekday.cmp(&b.weekday).then(a.hour.cmp(&b.hour)));
    let max_avg_ms = cells.iter().map(|c| c.avg_ms).max().unwrap_or(0);

    RhythmResponse {
        from: from.to_string(),
        to: to.to_string(),
        max_avg_ms,
        cells,
    }
}
