use serde::Serialize;

use super::forecast::finite;
use super::stats::{mean, pearson};

// ---------------------------------------------------------------------------
// 6. Disruption impact
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct DisruptionDay {
    pub date: String,
    pub disruptions: i64,
    pub notifications: i64,
    pub clipboard: i64,
    pub efficiency: f64,
    pub focus_ratio: f64,
    pub active_ms: i64,
    pub avg_dwell_ms: i64,
    pub session_count: i64,
}

#[derive(Debug, Serialize)]
pub struct DisruptionCorrelations {
    pub efficiency_vs_disruptions: f64,
    pub focus_ratio_vs_disruptions: f64,
    pub avg_dwell_vs_disruptions: f64,
    pub active_vs_disruptions: f64,
}

#[derive(Debug, Serialize)]
pub struct DisruptionBucket {
    pub count: usize,
    pub avg_efficiency: f64,
    pub avg_focus_ratio: f64,
}

#[derive(Debug, Serialize)]
pub struct DisruptionImpactResponse {
    #[serde(rename = "from")]
    pub from: String,
    #[serde(rename = "to")]
    pub to: String,
    pub n_days: usize,
    pub days: Vec<DisruptionDay>,
    pub correlations: DisruptionCorrelations,
    pub high_disruption_days: DisruptionBucket,
    pub low_disruption_days: DisruptionBucket,
    pub insight: String,
}

pub fn disruption_impact(
    mut days: Vec<DisruptionDay>,
    from: &str,
    to: &str,
) -> DisruptionImpactResponse {    days.sort_by(|a, b| a.date.cmp(&b.date));

    let disruptions: Vec<f64> = days.iter().map(|d| d.disruptions as f64).collect();
    let efficiency: Vec<f64> = days.iter().map(|d| d.efficiency).collect();
    let focus: Vec<f64> = days.iter().map(|d| d.focus_ratio).collect();
    let dwell: Vec<f64> = days.iter().map(|d| d.avg_dwell_ms as f64).collect();
    let active: Vec<f64> = days.iter().map(|d| d.active_ms as f64).collect();

    let correlations = DisruptionCorrelations {
        efficiency_vs_disruptions: pearson(&disruptions, &efficiency),
        focus_ratio_vs_disruptions: pearson(&disruptions, &focus),
        avg_dwell_vs_disruptions: pearson(&disruptions, &dwell),
        active_vs_disruptions: pearson(&disruptions, &active),
    };

    // Top / bottom quartile by disruption count, at least one day on each side.
    let n = days.len();
    let bucket_size = if n == 0 {
        0
    } else {
        ((n as f64 * 0.25).ceil() as usize).max(1).min(n)
    };
    let mut by_disruptions: Vec<&DisruptionDay> = days.iter().collect();
    by_disruptions.sort_by(|a, b| {
        b.disruptions
            .cmp(&a.disruptions)
            .then(a.date.cmp(&b.date))
    });
    let high_slice = &by_disruptions[..bucket_size.min(n)];
    let low_start = n.saturating_sub(bucket_size);
    let low_slice = &by_disruptions[low_start..];

    let bucket = |slice: &[&DisruptionDay]| -> DisruptionBucket {
        let eff: Vec<f64> = slice.iter().map(|d| d.efficiency).collect();
        let foc: Vec<f64> = slice.iter().map(|d| d.focus_ratio).collect();
        DisruptionBucket {
            count: slice.len(),
            avg_efficiency: finite(mean(&eff)),
            avg_focus_ratio: finite(mean(&foc)),
        }
    };
    let high = bucket(high_slice);
    let low = bucket(low_slice);

    let insight = build_insight(&high, &low, correlations.efficiency_vs_disruptions, n);

    DisruptionImpactResponse {
        from: from.to_string(),
        to: to.to_string(),
        n_days: n,
        days,
        correlations,
        high_disruption_days: high,
        low_disruption_days: low,
        insight,
    }
}

/// One factual sentence, no exclamation marks or emoji.
fn build_insight(
    high: &DisruptionBucket,
    low: &DisruptionBucket,
    corr: f64,
    n_days: usize,
) -> String {
    if n_days == 0 {
        return "No activity was recorded in this range.".to_string();
    }
    if high.count == 0 || low.count == 0 {
        return "Not enough days to compare interruption levels.".to_string();
    }
    let delta = low.avg_efficiency - high.avg_efficiency;
    if delta.abs() < 0.5 {
        return format!(
            "Days with the most interruptions score within 1 point of the quietest days ({} vs {}).",
            high.avg_efficiency.round() as i64,
            low.avg_efficiency.round() as i64
        );
    }
    if delta > 0.0 {
        let corr_note = if corr < -0.1 {
            " The correlation is negative."
        } else {
            ""
        };
        format!(
            "Days with the most interruptions score {} points lower on average ({} vs {}).{}",
            delta.round() as i64,
            high.avg_efficiency.round() as i64,
            low.avg_efficiency.round() as i64,
            corr_note
        )
    } else {
        format!(
            "Days with the most interruptions score {} points higher on average ({} vs {}).",
            (-delta).round() as i64,
            high.avg_efficiency.round() as i64,
            low.avg_efficiency.round() as i64
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disruption_impact_buckets_and_insight() {
        let day = |date: &str, disruptions: i64, efficiency: f64| DisruptionDay {
            date: date.to_string(),
            disruptions,
            notifications: disruptions,
            clipboard: 0,
            efficiency,
            focus_ratio: 0.4,
            active_ms: 8_000_000,
            avg_dwell_ms: 45_000,
            session_count: 300,
        };
        let days = vec![
            day("2026-09-01", 0, 90.0),
            day("2026-09-02", 1, 88.0),
            day("2026-09-03", 2, 86.0),
            day("2026-09-04", 10, 60.0),
        ];
        let r = disruption_impact(days, "2026-09-01", "2026-09-04");
        assert_eq!(r.n_days, 4);
        // ceil(4*0.25) = 1 day per bucket.
        assert_eq!(r.high_disruption_days.count, 1);
        assert_eq!(r.low_disruption_days.count, 1);
        assert_eq!(r.high_disruption_days.avg_efficiency, 60.0);
        assert_eq!(r.low_disruption_days.avg_efficiency, 90.0);
        assert!(r.correlations.efficiency_vs_disruptions < 0.0);
        assert!(r.insight.contains("30 points lower"), "{}", r.insight);
        assert!(!r.insight.contains('!'));
        // Empty input stays valid.
        let empty = disruption_impact(Vec::new(), "2026-09-01", "2026-09-04");
        assert_eq!(empty.n_days, 0);
        assert_eq!(empty.correlations.efficiency_vs_disruptions, 0.0);
        assert_eq!(empty.high_disruption_days.count, 0);
        assert!(!empty.insight.is_empty());
    }
}
