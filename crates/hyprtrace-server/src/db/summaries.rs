use crate::models::{DailyActivity, TodaySummary};
use rusqlite::params;

impl super::Database {
    pub fn today_summary(&self, date: &str) -> anyhow::Result<TodaySummary> {
        let total_active_ms: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(total_ms), 0) FROM daily_summary WHERE date = ?1",
            params![date],
            |row| row.get(0),
        )?;
        // Focus is derived from the day's sessions, exactly as the efficiency
        // score does it. Reading `daily_summary.focused_ms` here would show the
        // legacy per-session-threshold figure, i.e. "Focus time <1m" on the very
        // days the efficiency card now reports real focus for. See
        // `insights::focus`.
        let total_focused_ms: i64 = self
            .insight_sessions(date, date, 0)
            .map(|sessions| crate::insights::focus_total_ms(&sessions))
            .unwrap_or(0);

        let app_count: usize = self.conn.query_row(
            "SELECT COUNT(DISTINCT class) FROM daily_summary WHERE date = ?1",
            params![date],
            |row| row.get(0),
        )?;

        let session_count: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(session_count), 0) FROM daily_summary WHERE date = ?1",
            params![date],
            |row| row.get(0),
        )?;

        let top_apps = self.app_ranking(date, date, 5)?;

        let total_idle_ms: i64 = {
            // Idle time = gaps between consecutive sessions where the previous
            // session ended because of an idle/away timeout. Small gaps caused
            // by normal app switches (activity_state = active/focused) are NOT
            // idle. This also avoids the old `span - active` formula, which
            // accumulated every tiny switch gap and could drift with clock
            // corrections or overlapping sessions.
            self.conn
                .query_row(
                    "WITH ordered AS (
                        SELECT started_at,
                               ended_at,
                               activity_state,
                               date_local,
                               LAG(ended_at) OVER (ORDER BY started_at) AS prev_end,
                               LAG(activity_state) OVER (ORDER BY started_at) AS prev_state
                        FROM sessions
                        WHERE ended_at IS NOT NULL
                    )
                    SELECT COALESCE(SUM(
                        CAST((julianday(started_at) - julianday(prev_end)) * 86400000 AS INTEGER)
                    ), 0)
                    FROM ordered
                    WHERE date_local = ?1
                      AND prev_end IS NOT NULL
                      AND started_at > prev_end
                      AND prev_state IN ('idle', 'away')",
                    params![date],
                    |row| row.get(0),
                )
                .unwrap_or(0)
        };

        Ok(TodaySummary {
            date: date.to_string(),
            total_active_ms,
            total_idle_ms,
            total_focused_ms,
            app_count,
            session_count,
            top_apps,
        })
    }

    /// Zero-filled daily activity totals for the trailing `days` days (ending
    /// today, inclusive), ordered oldest → newest. Drives the yearly heatmap.
    pub fn daily_activity(&self, days: i64) -> anyhow::Result<Vec<DailyActivity>> {
        let days = days.clamp(7, 730);
        let today = chrono::Local::now().date_naive();
        let start = today - chrono::Duration::days(days - 1);
        let start_str = start.format("%Y-%m-%d").to_string();
        let end_str = today.format("%Y-%m-%d").to_string();

        let mut stmt = self.conn.prepare(
            "SELECT date,
                    COALESCE(SUM(total_ms), 0),
                    COALESCE(SUM(focused_ms), 0),
                    COALESCE(SUM(session_count), 0)
             FROM daily_summary
             WHERE date BETWEEN ?1 AND ?2
             GROUP BY date",
        )?;
        let rows = stmt.query_map(params![start_str, end_str], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?;

        let mut by_date = std::collections::HashMap::new();
        for r in rows {
            let (date, total_ms, focused_ms, session_count) = r?;
            by_date.insert(date, (total_ms, focused_ms, session_count));
        }

        let mut out = Vec::with_capacity(days as usize);
        let mut cursor = start;
        for _ in 0..days {
            let date = cursor.format("%Y-%m-%d").to_string();
            let (total_ms, focused_ms, session_count) =
                by_date.get(&date).copied().unwrap_or((0, 0, 0));
            out.push(DailyActivity {
                date,
                total_ms,
                focused_ms,
                session_count,
            });
            cursor += chrono::Duration::days(1);
        }
        Ok(out)
    }
}
