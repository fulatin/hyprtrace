use crate::models::CurrentStatus;
use rusqlite::params;

impl super::Database {
    /// Compact live status for lightweight consumers (e.g. Waybar).
    pub fn current_status(&self) -> anyhow::Result<CurrentStatus> {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();

        let current_app: String = self.conn.query_row(
            "SELECT class FROM sessions WHERE ended_at IS NULL ORDER BY started_at DESC LIMIT 1",
            [],
            |r| r.get(0),
        ).unwrap_or_else(|_| "—".to_string());

        let current_session_min: i64 = self.conn.query_row(
            "SELECT COALESCE(CAST((julianday('now') - julianday(started_at)) * 1440 AS INTEGER), 0)
             FROM sessions WHERE ended_at IS NULL ORDER BY started_at DESC LIMIT 1",
            [],
            |r| r.get(0),
        ).unwrap_or(0);

        let today_ms: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(SUM(total_ms), 0) FROM daily_summary WHERE date = ?1",
                params![today],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let today_focused_ms: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(SUM(focused_ms), 0) FROM daily_summary WHERE date = ?1",
                params![today],
                |r| r.get(0),
            )
            .unwrap_or(0);

        // First enabled goal (target) for a progress bar.
        let (goal_name, goal_ms): (Option<String>, i64) = self
            .conn
            .query_row(
                "SELECT name, daily_target_ms FROM goals WHERE enabled = 1 ORDER BY id LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map(|(n, m): (String, i64)| (Some(n), m))
            .unwrap_or((None, 0));

        let today_pct_goal = if goal_ms > 0 {
            (today_ms as f64 / goal_ms as f64) * 100.0
        } else {
            0.0
        };

        let efficiency_score = self.efficiency_score(&today).ok().map(|e| e.score);

        Ok(CurrentStatus {
            current_app,
            current_session_min,
            today_ms,
            today_focused_ms,
            today_pct_goal,
            goal_name,
            efficiency_score,
        })
    }
}
