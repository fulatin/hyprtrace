use serde::Serialize;
use std::collections::HashMap;

use super::forecast::finite;
use super::session::InsightSession;
use super::stats::{median, ols};

// ---------------------------------------------------------------------------
// 4. Fragmentation
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct FragmentationDay {
    pub date: String,
    pub session_count: i64,
    pub switch_count: i64,
    pub avg_dwell_ms: i64,
    pub median_dwell_ms: i64,
    pub short_session_ratio: f64,
    pub longest_focus_ms: i64,
    pub focus_blocks: i64,
    pub active_ms: i64,
    pub context_switch_cost_ms: i64,
}

#[derive(Debug, Serialize)]
pub struct WorstDay {
    pub date: String,
    pub switch_count: i64,
    pub avg_dwell_ms: i64,
}

#[derive(Debug, Serialize)]
pub struct FragmentationResponse {
    #[serde(rename = "from")]
    pub from: String,
    #[serde(rename = "to")]
    pub to: String,
    pub days: Vec<FragmentationDay>,
    pub avg_dwell_ms: i64,
    pub median_dwell_ms: i64,
    pub avg_switch_count: i64,
    pub short_session_ratio: f64,
    pub switches_per_hour: f64,
    pub trend_slope_ms_per_day: f64,
    pub worst_day: Option<WorstDay>,
}

/// Sessions shorter than this count as "short" (fragmented attention).
const SHORT_SESSION_MS: i64 = 60_000;
/// A focus block is a run of same-class sessions spanning at least 15 minutes.
const FOCUS_BLOCK_MS: i64 = 15 * 60_000;
/// Re-orientation cost per switch is capped at one minute of the previous dwell.
const SWITCH_COST_CAP_MS: i64 = 60_000;
/// Fraction of the pre-switch dwell charged as lost re-orientation time.
const SWITCH_COST_FACTOR: f64 = 0.5;

/// Cost of one switch: half of the previous dwell, capped at 60s.
///
/// Rationale: leaving an app and coming back costs the user time to rebuild
/// context. Charging half the previous dwell (bounded by a minute) keeps the
/// estimate conservative and independent of session length for long sessions.
pub fn switch_cost_ms(prev_dwell_ms: i64) -> i64 {
    let capped = prev_dwell_ms.max(0).min(SWITCH_COST_CAP_MS);
    (capped as f64 * SWITCH_COST_FACTOR).round() as i64
}

pub fn fragmentation(sessions: &[InsightSession], from: &str, to: &str) -> FragmentationResponse {
    // Group by local date, preserving the chronological order of the input.
    let mut by_day: Vec<(String, Vec<&InsightSession>)> = Vec::new();
    let mut day_index: HashMap<String, usize> = HashMap::new();
    for s in sessions {
        match day_index.get(&s.date_local) {
            Some(&i) => by_day[i].1.push(s),
            None => {
                day_index.insert(s.date_local.clone(), by_day.len());
                by_day.push((s.date_local.clone(), vec![s]));
            }
        }
    }
    by_day.sort_by(|a, b| a.0.cmp(&b.0));

    let mut days = Vec::with_capacity(by_day.len());
    let mut all_dwells: Vec<f64> = Vec::new();
    let mut total_switches = 0i64;
    let mut total_active = 0i64;
    let mut total_sessions = 0i64;
    let mut total_short = 0i64;

    for (date, day_sessions) in by_day.iter() {
        let session_count = day_sessions.len() as i64;
        let mut dwells: Vec<f64> = day_sessions.iter().map(|s| s.dwell_ms() as f64).collect();
        let active_ms: i64 = day_sessions.iter().map(|s| s.dwell_ms()).sum();

        let mut switch_count = 0i64;
        let mut cost = 0i64;
        for pair in day_sessions.windows(2) {
            if pair[0].class != pair[1].class {
                switch_count += 1;
                cost += switch_cost_ms(pair[0].dwell_ms());
            }
        }

        let short = day_sessions
            .iter()
            .filter(|s| s.dwell_ms() < SHORT_SESSION_MS)
            .count() as i64;

        // Focus blocks: consecutive same-class sessions whose combined duration
        // reaches 15 minutes. A single long session is its own block.
        let mut focus_blocks = 0i64;
        let mut run_class: Option<&str> = None;
        let mut run_ms = 0i64;
        for s in day_sessions.iter() {
            if run_class == Some(s.class.as_str()) {
                run_ms += s.dwell_ms();
            } else {
                if run_ms >= FOCUS_BLOCK_MS {
                    focus_blocks += 1;
                }
                run_class = Some(s.class.as_str());
                run_ms = s.dwell_ms();
            }
        }
        if run_ms >= FOCUS_BLOCK_MS {
            focus_blocks += 1;
        }

        let longest_focus_ms = {
            // Longest unbroken same-class run, reported as the run duration.
            let mut best = 0i64;
            let mut cur_class: Option<&str> = None;
            let mut cur = 0i64;
            for s in day_sessions.iter() {
                if cur_class == Some(s.class.as_str()) {
                    cur += s.dwell_ms();
                } else {
                    best = best.max(cur);
                    cur_class = Some(s.class.as_str());
                    cur = s.dwell_ms();
                }
            }
            best.max(cur)
        };

        let day_avg_dwell = if session_count > 0 {
            active_ms / session_count
        } else {
            0
        };
        days.push(FragmentationDay {
            date: date.clone(),
            session_count,
            switch_count,
            avg_dwell_ms: day_avg_dwell,
            median_dwell_ms: median(&mut dwells) as i64,
            short_session_ratio: if session_count > 0 {
                short as f64 / session_count as f64
            } else {
                0.0
            },
            longest_focus_ms,
            focus_blocks,
            active_ms,
            context_switch_cost_ms: cost,
        });

        all_dwells.extend(day_sessions.iter().map(|s| s.dwell_ms() as f64));
        total_switches += switch_count;
        total_active += active_ms;
        total_sessions += session_count;
        total_short += short;
    }

    let avg_dwell_ms = if total_sessions > 0 {
        total_active / total_sessions
    } else {
        0
    };
    let median_dwell_ms = median(&mut all_dwells) as i64;
    let avg_switch_count = if days.is_empty() {
        0
    } else {
        total_switches / days.len() as i64
    };
    let short_session_ratio = if total_sessions > 0 {
        total_short as f64 / total_sessions as f64
    } else {
        0.0
    };
    let active_hours = total_active as f64 / 3_600_000.0;
    let switches_per_hour = if active_hours > f64::EPSILON {
        total_switches as f64 / active_hours
    } else {
        0.0
    };

    let trend_slope_ms_per_day = if days.len() >= 2 {
        let y: Vec<f64> = days.iter().map(|d| d.avg_dwell_ms as f64).collect();
        finite(ols(&y).slope)
    } else {
        0.0
    };

    let worst_day = days
        .iter()
        .max_by(|a, b| {
            a.switch_count
                .cmp(&b.switch_count)
                .then(b.avg_dwell_ms.cmp(&a.avg_dwell_ms))
                .then(a.date.cmp(&b.date))
        })
        .map(|d| WorstDay {
            date: d.date.clone(),
            switch_count: d.switch_count,
            avg_dwell_ms: d.avg_dwell_ms,
        });

    FragmentationResponse {
        from: from.to_string(),
        to: to.to_string(),
        days,
        avg_dwell_ms,
        median_dwell_ms,
        avg_switch_count,
        short_session_ratio,
        switches_per_hour: finite(switches_per_hour),
        trend_slope_ms_per_day,
        worst_day,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::insights::test_support::session;

    #[test]
    fn fragmentation_cost_helper_is_half_of_capped_dwell() {
        assert_eq!(switch_cost_ms(0), 0);
        assert_eq!(switch_cost_ms(-500), 0);
        assert_eq!(switch_cost_ms(10_000), 5_000);
        // 60s cap: 120s dwell costs the same as 60s dwell.
        assert_eq!(switch_cost_ms(60_000), 30_000);
        assert_eq!(switch_cost_ms(120_000), 30_000);
        assert_eq!(switch_cost_ms(5_400_000), 30_000);
    }
    #[test]
    fn fragmentation_counts_switches_focus_blocks_and_short_sessions() {
        let sessions = vec![
            session("kitty", "2026-09-01T00:00:00Z", "2026-09-01T00:20:00Z", 1_200_000, "2026-09-01"),
            session("kitty", "2026-09-01T00:20:00Z", "2026-09-01T00:25:00Z", 300_000, "2026-09-01"),
            session("firefox", "2026-09-01T00:25:00Z", "2026-09-01T00:25:10Z", 10_000, "2026-09-01"),
            session("kitty", "2026-09-01T00:25:10Z", "2026-09-01T00:30:00Z", 290_000, "2026-09-01"),
        ];
        let r = fragmentation(&sessions, "2026-09-01", "2026-09-01");
        assert_eq!(r.days.len(), 1);
        let d = &r.days[0];
        assert_eq!(d.session_count, 4);
        // k→k is not a switch; k→f and f→k are.
        assert_eq!(d.switch_count, 2);
        assert_eq!(d.short_session_ratio, 0.25);
        // First two kitty sessions = 25 min ≥ 15 min → one focus block.
        assert_eq!(d.focus_blocks, 1);
        assert_eq!(d.longest_focus_ms, 1_500_000);
        assert_eq!(d.active_ms, 1_800_000);
        // cost = min(20min,60s)*0.5 + min(10s,60s)*0.5 = 30_000 + 5_000
        assert_eq!(d.context_switch_cost_ms, 35_000);
        assert_eq!(r.worst_day.as_ref().unwrap().date, "2026-09-01");
        // Empty input stays valid.
        let empty = fragmentation(&[], "2026-09-01", "2026-09-01");
        assert_eq!(empty.avg_dwell_ms, 0);
        assert_eq!(empty.switches_per_hour, 0.0);
        assert!(empty.worst_day.is_none());
    }
}
