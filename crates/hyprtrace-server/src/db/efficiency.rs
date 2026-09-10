use crate::models::EfficiencyScore;
use rusqlite::params;

impl super::Database {
    /// Compute a 0-100 efficiency score for a single day.
    ///
    /// Factors (each mapped to a sub-score, summed):
    ///   - Focus ratio (focused_ms / active_ms)                    → up to 40
    ///   - Session fragmentation (avg session length)              → up to 30
    ///   - Late-night usage (23:00-05:59 share of active time)     → up to 15
    ///   - Interruptions (notification count, 0-20)                → up to 15
    pub fn efficiency_score(&self, date: &str) -> anyhow::Result<EfficiencyScore> {
        let total_active_ms: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(SUM(total_ms), 0) FROM daily_summary WHERE date = ?1",
                params![date],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Focus is DERIVED from the day's sessions, not read from
        // `daily_summary.focused_ms`: that column holds the legacy per-session
        // threshold figure, which is 0 on any day built out of short sessions —
        // i.e. on most real days, which is why the dashboard reported "Focus
        // time <1m". See `insights::focus` for the rule.
        let total_focused_ms: i64 = self
            .insight_sessions(date, date, 0)
            .map(|sessions| crate::insights::focus_total_ms(&sessions))
            .unwrap_or(0);

        let session_count: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(SUM(session_count), 0) FROM daily_summary WHERE date = ?1",
                params![date],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Late-night share from hourly_summary (local hours 23, 0..6).
        let late_night_ms: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(SUM(total_ms), 0) FROM hourly_summary
             WHERE date = ?1 AND hour >= 23 OR date = ?1 AND hour <= 5",
                params![date, date],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let disruption_count: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(COUNT(*), 0) FROM disruptions
             WHERE date_local = ?1 AND kind = 'notification'",
                params![date],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let focus_ratio = if total_active_ms > 0 {
            total_focused_ms as f64 / total_active_ms as f64
        } else {
            0.0
        };
        let avg_session_secs = if session_count > 0 {
            total_active_ms as f64 / session_count as f64 / 1000.0
        } else {
            0.0
        };
        let late_night_pct = if total_active_ms > 0 {
            (late_night_ms as f64 / total_active_ms as f64) * 100.0
        } else {
            0.0
        };

        let focus_score = (focus_ratio * 40.0).clamp(0.0, 40.0);
        // Ideal avg session 15-90 min → full fragmentation score.
        let frag_score = if avg_session_secs >= 15.0 * 60.0 && avg_session_secs <= 90.0 * 60.0 {
            30.0
        } else if avg_session_secs < 15.0 * 60.0 {
            // Very fragmented: scale from 0 (0s) to 30 (15min).
            (avg_session_secs / (15.0 * 60.0)) * 30.0
        } else {
            // Long sessions: gently taper.
            30.0_f64 - ((avg_session_secs - 90.0 * 60.0) / (180.0 * 60.0)) * 10.0
        };
        let late_score = (15.0 * (1.0 - late_night_pct / 100.0)).clamp(0.0, 15.0);
        let disruption_score = (15.0 * (1.0 - (disruption_count as f64) / 20.0)).clamp(0.0, 15.0);

        let score =
            (focus_score + frag_score.max(0.0) + late_score + disruption_score).round() as i64;

        Ok(EfficiencyScore {
            date: date.to_string(),
            score: score.clamp(0, 100),
            focus_ratio,
            avg_session_secs,
            late_night_pct,
            disruption_count,
            total_active_ms,
        })
    }
}
