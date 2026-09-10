use rusqlite::params;

use super::Goal;

impl super::Database {
    pub fn goals(&self) -> anyhow::Result<Vec<Goal>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, target_type, target_key, daily_target_ms, enabled FROM goals",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Goal {
                id: Some(row.get(0)?),
                name: row.get(1)?,
                target_type: row.get(2)?,
                target_key: row.get(3)?,
                daily_target_ms: row.get(4)?,
                enabled: row.get::<_, i64>(5)? != 0,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Today's active milliseconds for a goal. "all" = total across classes.
    pub fn today_active_for_goal(&self, goal: &Goal) -> i64 {
        // LOCAL today: must match the local-date buckets end_session writes,
        // otherwise goal progress misses same-day rows (or reads stale ones).
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let sql = if goal.target_type == "class" {
            "SELECT COALESCE(SUM(total_ms), 0) FROM daily_summary WHERE date = ?1 AND class = ?2"
        } else {
            "SELECT COALESCE(SUM(total_ms), 0) FROM daily_summary WHERE date = ?1"
        };
        let params = if goal.target_type == "class" {
            params![today, goal.target_key.as_deref().unwrap_or("")]
        } else {
            params![today]
        };
        self.conn
            .query_row(sql, params, |row| row.get(0))
            .unwrap_or(0)
    }

    /// Duration (ms) the current focused session has been running.
    pub fn current_focused_duration_ms(&self) -> i64 {
        let result: rusqlite::Result<(i64, String)> = self.conn.query_row(
            "SELECT id, activity_state FROM sessions
             WHERE ended_at IS NULL ORDER BY started_at DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        );
        match result {
            Ok((session_id, state)) if state == "focused" => {
                self.session_elapsed_ms(session_id).unwrap_or(0).max(0)
            }
            _ => 0,
        }
    }
}
