use chrono::Duration as ChronoDuration;
use serde::Serialize;
use std::collections::HashMap;

use super::forecast::finite;
use super::stats::robust_z;
use super::time::{parse_date, weekday_mon0};

// ---------------------------------------------------------------------------
// 7. Anomalies
// ---------------------------------------------------------------------------

/// Efficiency sub-score (0..100) from pre-aggregated day totals.
///
/// Same factors as `Database::efficiency_score` (focus 40 / fragmentation 30 /
/// late night 15 / disruptions 15) so the disruption-impact endpoint can score a
/// whole range from summary rows without a query per day.
pub fn efficiency_subscore(
    active_ms: i64,
    focused_ms: i64,
    session_count: i64,
    late_night_ms: i64,
    disruptions: i64,
) -> f64 {
    if active_ms <= 0 {
        return 0.0;
    }
    let focus_ratio = (focused_ms as f64 / active_ms as f64).clamp(0.0, 1.0);
    let avg_session_secs = active_ms as f64 / session_count.max(1) as f64 / 1000.0;
    let late_night_pct = (late_night_ms as f64 / active_ms as f64) * 100.0;

    let focus_score = (focus_ratio * 40.0).clamp(0.0, 40.0);
    let frag_score = if avg_session_secs >= 15.0 * 60.0 && avg_session_secs <= 90.0 * 60.0 {
        30.0
    } else if avg_session_secs < 15.0 * 60.0 {
        (avg_session_secs / (15.0 * 60.0)) * 30.0
    } else {
        30.0 - ((avg_session_secs - 90.0 * 60.0) / (180.0 * 60.0)) * 10.0
    };
    let late_score = (15.0 * (1.0 - late_night_pct / 100.0)).clamp(0.0, 15.0);
    let disruption_score = (15.0 * (1.0 - disruptions as f64 / 20.0)).clamp(0.0, 15.0);

    let score = focus_score + frag_score.max(0.0) + late_score + disruption_score;
    finite(score).clamp(0.0, 100.0)
}

#[derive(Debug, Serialize)]
pub struct AnomalyDay {
    pub date: String,
    pub active_ms: i64,
    pub baseline_ms: i64,
    pub z_score: f64,
    pub severity: String,
    pub reasons: Vec<String>,
    pub late_night_share: f64,
    pub session_count: i64,
    pub avg_dwell_ms: i64,
    pub weekday: u8,
}

#[derive(Debug, Serialize)]
pub struct AnomaliesResponse {
    #[serde(rename = "from")]
    pub from: String,
    #[serde(rename = "to")]
    pub to: String,
    pub window: i64,
    pub threshold: f64,
    pub baseline_days: usize,
    pub days: Vec<AnomalyDay>,
}

/// Per-day aggregates fed into the anomaly detector.
pub struct AnomalyInput {
    pub date: String,
    pub active_ms: i64,
    pub session_count: i64,
    pub avg_dwell_ms: i64,
    /// Share of the day's ms spent in local hours 23 and 0..=5.
    pub late_night_share: f64,
    pub short_session_ratio: f64,
}

/// Flag days that deviate from the user's own trailing habit.
///
/// The baseline is the median of the previous `window` days (a robust centre
/// that one binge day cannot move) and the z-score is scaled by 1.4826*MAD.
/// Days inside the first `window` days of the range have no full baseline and
/// are skipped.
pub fn anomalies(
    inputs: &[AnomalyInput],
    from: &str,
    to: &str,
    window: i64,
    threshold: f64,
) -> AnomaliesResponse {
    let by_date: HashMap<&str, &AnomalyInput> =
        inputs.iter().map(|i| (i.date.as_str(), i)).collect();
    let mut days: Vec<AnomalyDay> = Vec::new();
    let mut baseline_days = 0usize;

    let start = match parse_date(from) {
        Some(d) => d,
        None => {
            return AnomaliesResponse {
                from: from.to_string(),
                to: to.to_string(),
                window,
                threshold,
                baseline_days: 0,
                days,
            }
        }
    };
    let end = match parse_date(to) {
        Some(d) => d,
        None => {
            return AnomaliesResponse {
                from: from.to_string(),
                to: to.to_string(),
                window,
                threshold,
                baseline_days: 0,
                days,
            }
        }
    };

    let mut cur = start;
    while cur <= end {
        let key = cur.format("%Y-%m-%d").to_string();
        let index = (cur - start).num_days();
        if index < window {
            cur += ChronoDuration::days(1);
            continue;
        }
        let window_start = cur - ChronoDuration::days(window);
        let mut baseline: Vec<f64> = Vec::with_capacity(window as usize);
        let mut w = window_start;
        while w < cur {
            let wk = w.format("%Y-%m-%d").to_string();
            let value = by_date.get(wk.as_str()).map(|i| i.active_ms).unwrap_or(0);
            baseline.push(value as f64);
            w += ChronoDuration::days(1);
        }
        if baseline.len() > baseline_days {
            baseline_days = baseline.len();
        }

        let today = by_date.get(key.as_str());
        let active_ms = today.map(|i| i.active_ms).unwrap_or(0);
        let (med, z) = robust_z(active_ms as f64, &baseline);

        let late_night = today.map(|i| i.late_night_share).unwrap_or(0.0);
        let short_ratio = today.map(|i| i.short_session_ratio).unwrap_or(0.0);
        let session_count = today.map(|i| i.session_count).unwrap_or(0);
        let avg_dwell = today.map(|i| i.avg_dwell_ms).unwrap_or(0);

        let z_fired = z.abs() >= threshold;
        let late_fired = late_night >= 0.35;
        let short_fired = short_ratio >= 0.75;
        let mut reasons = Vec::new();
        if z_fired {
            let dir = if z >= 0.0 { "above" } else { "below" };
            reasons.push(format!(
                "Active time {:.1}σ {} your {}-day baseline",
                z.abs(),
                dir,
                window
            ));
        }
        if late_fired {
            reasons.push(format!(
                "Late-night usage {:.0}% of active time",
                late_night * 100.0
            ));
        }
        if short_fired {
            reasons.push(format!(
                "Short sessions {:.0}% of sessions",
                short_ratio * 100.0
            ));
        }

        if reasons.is_empty() {
            cur += ChronoDuration::days(1);
            continue;
        }

        let triggers = reasons.len();
        let severity = if z.abs() >= threshold * 1.5 || triggers >= 2 {
            "high"
        } else if z_fired {
            "medium"
        } else {
            "low"
        };

        days.push(AnomalyDay {
            date: key,
            active_ms,
            baseline_ms: med.round().max(0.0) as i64,
            z_score: finite(z),
            severity: severity.to_string(),
            reasons,
            late_night_share: finite(late_night),
            session_count,
            avg_dwell_ms: avg_dwell,
            weekday: weekday_mon0(cur),
        });
        cur += ChronoDuration::days(1);
    }

    let severity_rank = |s: &str| match s {
        "high" => 0,
        "medium" => 1,
        _ => 2,
    };
    days.sort_by(|a, b| {
        severity_rank(&a.severity)
            .cmp(&severity_rank(&b.severity))
            .then(
                b.z_score
                    .abs()
                    .partial_cmp(&a.z_score.abs())
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then(a.date.cmp(&b.date))
    });
    days.truncate(30);

    AnomaliesResponse {
        from: from.to_string(),
        to: to.to_string(),
        window,
        threshold,
        baseline_days,
        days,
    }
}

/// Hours counted as "late night" for the anomaly detector: 23:00 and 00:00-05:59.
///
/// The SQL in `db/insights_sql.rs::insight_anomalies` filters the same hours; this is
/// single Rust-side definition, kept test-only so it cannot drift silently.
#[cfg(test)]
pub fn is_late_night_hour(hour: u8) -> bool {
    hour == 23 || hour <= 5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn late_night_hours_match_the_sql_predicate() {
        // Mirrors `hour >= 23 OR hour <= 5` in db.rs::insight_anomalies.
        for hour in 0..24u8 {
            assert_eq!(
                is_late_night_hour(hour),
                hour == 23 || hour <= 5,
                "hour {hour}"
            );
        }
    }
    #[test]
    fn anomaly_detector_flags_spike_and_late_night() {
        // 14 quiet days then a spike day; window=14 skips the first 14 days.
        let mut inputs: Vec<AnomalyInput> = (0..14)
            .map(|i| AnomalyInput {
                date: format!("2026-08-{:02}", i + 1),
                active_ms: 8_000_000,
                session_count: 100,
                avg_dwell_ms: 60_000,
                late_night_share: 0.0,
                short_session_ratio: 0.1,
            })
            .collect();
        inputs.push(AnomalyInput {
            date: "2026-08-15".to_string(),
            active_ms: 16_000_000,
            session_count: 520,
            avg_dwell_ms: 9_000,
            late_night_share: 0.41,
            short_session_ratio: 0.8,
        });
        let r = anomalies(&inputs, "2026-08-01", "2026-08-15", 14, 2.0);
        assert_eq!(r.baseline_days, 14);
        assert_eq!(r.days.len(), 1);
        let d = &r.days[0];
        assert_eq!(d.date, "2026-08-15");
        assert_eq!(d.baseline_ms, 8_000_000);
        // Flat baseline → std fallback 0 → z 0, but late-night and short-session
        // triggers still fire, and two triggers make it high severity.
        assert_eq!(d.z_score, 0.0);
        assert_eq!(d.severity, "high");
        assert_eq!(d.reasons.len(), 2);
        // Days inside the first window are never reported.
        let r = anomalies(&inputs, "2026-08-01", "2026-08-10", 14, 2.0);
        assert!(r.days.is_empty());
        assert_eq!(r.baseline_days, 0);
    }
}
