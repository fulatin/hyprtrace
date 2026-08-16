use anyhow::Context;
use chrono::Timelike;
use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;

pub struct Database {
    conn: Connection,
    focused_threshold_ms: i64,
    /// Monotonic start instants for sessions started by this daemon process.
    /// Wall-clock time can jump (dual-boot with Windows, NTP corrections), but
    /// `Instant` is monotonic and keeps durations accurate (and never negative).
    monotonic_starts: Mutex<HashMap<i64, Instant>>,
}

#[derive(Debug, Clone)]
pub struct Goal {
    pub id: Option<i64>,
    pub name: String,
    pub target_type: String, // "all" | "class"
    pub target_key: Option<String>,
    pub daily_target_ms: i64,
    pub enabled: bool,
}

impl Database {
    pub fn open(path: &Path, focused_threshold_seconds: u64) -> anyhow::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("Failed to open database: {:?}", path))?;

        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA busy_timeout=5000;
             PRAGMA foreign_keys=ON;",
        )?;

        Ok(Self {
            conn,
            focused_threshold_ms: (focused_threshold_seconds * 1000) as i64,
            monotonic_starts: Mutex::new(HashMap::new()),
        })
    }

    pub fn migrate(&self) -> anyhow::Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                class       TEXT NOT NULL,
                title       TEXT NOT NULL DEFAULT '',
                workspace   TEXT,
                started_at  TEXT NOT NULL,
                ended_at    TEXT,
                duration_ms INTEGER DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS daily_summary (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                date          TEXT NOT NULL,
                class         TEXT NOT NULL,
                total_ms      INTEGER NOT NULL DEFAULT 0,
                session_count INTEGER NOT NULL DEFAULT 0,
                UNIQUE(date, class)
            );

            CREATE TABLE IF NOT EXISTS ai_conversations (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at TEXT NOT NULL,
                role       TEXT NOT NULL,
                content    TEXT NOT NULL,
                model      TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_sessions_class ON sessions(class);
            CREATE INDEX IF NOT EXISTS idx_sessions_started ON sessions(started_at);
            CREATE INDEX IF NOT EXISTS idx_daily_summary_date ON daily_summary(date);",
        )?;

        self.migrate_v2()?;
        self.migrate_v3()?;
        self.migrate_v4()?;
        self.migrate_v5()?;
        self.migrate_v6()?;
        Ok(())
    }

    /// Repair sessions/events that were recorded with a negative duration when
    /// the wall clock jumped backwards (e.g. dual-boot with Windows leaving the
    /// RTC in local time), and rebuild the summary tables from the repaired
    /// sessions so no negative totals remain.
    fn migrate_v6(&self) -> anyhow::Result<()> {
        // SQLite MAX(duration_ms, 0) handles NULL as NULL, so clamp only the
        // negative rows explicitly.
        let sessions_repaired = self.conn.execute(
            "UPDATE sessions SET duration_ms = 0, focused_ms = MAX(COALESCE(focused_ms, 0), 0)
             WHERE duration_ms < 0",
            [],
        )?;
        let events_repaired = self.conn.execute(
            "UPDATE activity_events SET duration_ms = 0 WHERE duration_ms < 0",
            [],
        )?;
        if sessions_repaired > 0 || events_repaired > 0 {
            log::warn!(
                "Clamped {} negative session duration(s) and {} negative activity_event duration(s) to 0",
                sessions_repaired,
                events_repaired
            );
        }

        // Rebuild only when something was actually wrong; otherwise keep the
        // incremental summaries untouched (and avoid a full table rebuild on
        // every daemon start).
        let negative_summary_rows: i64 = self.conn.query_row(
            "SELECT (SELECT COUNT(*) FROM daily_summary WHERE total_ms < 0 OR focused_ms < 0)
                  + (SELECT COUNT(*) FROM hourly_summary WHERE total_ms < 0 OR focused_ms < 0)",
            [],
            |row| row.get(0),
        )?;

        if sessions_repaired > 0 || events_repaired > 0 || negative_summary_rows > 0 {
            self.rebuild_summaries_from_sessions()?;
        }
        Ok(())
    }

    /// Recompute daily_summary and hourly_summary from the `sessions` table.
    /// Used by migrate_v6 to clean up summaries polluted by negative durations.
    fn rebuild_summaries_from_sessions(&self) -> anyhow::Result<()> {
        self.conn.execute("DELETE FROM daily_summary", [])?;
        self.conn.execute(
            "INSERT INTO daily_summary (date, class, total_ms, session_count, focused_ms, focused_session_count)
             SELECT date(started_at),
                    class,
                    SUM(duration_ms),
                    COUNT(*),
                    SUM(COALESCE(focused_ms, 0)),
                    SUM(CASE WHEN COALESCE(focused_ms, 0) > 0 THEN 1 ELSE 0 END)
             FROM sessions
             WHERE ended_at IS NOT NULL AND duration_ms >= 0
             GROUP BY date(started_at), class",
            [],
        )?;

        self.conn.execute("DELETE FROM hourly_summary", [])?;

        let mut stmt = self.conn.prepare(
            "SELECT started_at, class, duration_ms, COALESCE(focused_ms, 0)
             FROM sessions
             WHERE ended_at IS NOT NULL AND duration_ms >= 0",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?;

        let mut agg: HashMap<(String, u8, String), (i64, i64, i64)> = HashMap::new();
        for r in rows {
            let (started_at, class, duration_ms, focused_ms) = r?;
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&started_at) {
                let date = started_at[..10].to_string();
                let hour = dt.with_timezone(&chrono::Local).hour() as u8;
                let entry = agg.entry((date, hour, class)).or_insert((0, 0, 0));
                entry.0 += duration_ms;
                entry.1 += 1;
                entry.2 += focused_ms;
            }
        }

        for ((date, hour, class), (total_ms, session_count, focused_ms)) in agg {
            self.conn.execute(
                "INSERT INTO hourly_summary (date, hour, class, total_ms, session_count, focused_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![date, hour as i64, class, total_ms, session_count, focused_ms],
            )?;
        }

        Ok(())
    }

    fn migrate_v5(&self) -> anyhow::Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS goals (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                name            TEXT NOT NULL,
                target_type     TEXT NOT NULL DEFAULT 'all',
                target_key      TEXT,
                daily_target_ms INTEGER NOT NULL,
                enabled         INTEGER NOT NULL DEFAULT 1
            );",
        )?;
        Ok(())
    }

    fn migrate_v4(&self) -> anyhow::Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS disruptions (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                kind       TEXT NOT NULL,
                app        TEXT,
                summary    TEXT,
                occurred_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_disruptions_at ON disruptions(occurred_at);
            CREATE INDEX IF NOT EXISTS idx_disruptions_kind ON disruptions(kind);",
        )?;
        Ok(())
    }

    fn migrate_v3(&self) -> anyhow::Result<()> {
        self.add_column_if_missing("sessions", "pid", "INTEGER", "NULL")?;

        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS app_resources (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id INTEGER,
                class      TEXT NOT NULL,
                sampled_at TEXT NOT NULL,
                cpu_pct    REAL,
                mem_kb     INTEGER,
                FOREIGN KEY (session_id) REFERENCES sessions(id)
            );

            CREATE INDEX IF NOT EXISTS idx_app_resources_class ON app_resources(class);
            CREATE INDEX IF NOT EXISTS idx_app_resources_session ON app_resources(session_id);",
        )?;
        Ok(())
    }

    fn migrate_v2(&self) -> anyhow::Result<()> {
        self.add_column_if_missing("sessions", "activity_state", "TEXT", "'active'")?;
        self.add_column_if_missing("sessions", "focused_ms", "INTEGER", "0")?;

        self.add_column_if_missing("daily_summary", "focused_ms", "INTEGER", "0")?;
        self.add_column_if_missing("daily_summary", "focused_session_count", "INTEGER", "0")?;

        self.add_column_if_missing("ai_conversations", "complete", "INTEGER", "1")?;

        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS activity_events (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id  INTEGER,
                state       TEXT NOT NULL,
                started_at  TEXT NOT NULL,
                ended_at    TEXT,
                duration_ms INTEGER DEFAULT 0,
                FOREIGN KEY (session_id) REFERENCES sessions(id)
            );

            CREATE TABLE IF NOT EXISTS hourly_summary (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                date          TEXT NOT NULL,
                hour          INTEGER NOT NULL,
                class         TEXT NOT NULL,
                total_ms      INTEGER NOT NULL DEFAULT 0,
                session_count INTEGER NOT NULL DEFAULT 0,
                focused_ms    INTEGER NOT NULL DEFAULT 0,
                UNIQUE(date, hour, class)
            );

            CREATE INDEX IF NOT EXISTS idx_hourly_summary_date ON hourly_summary(date);
            CREATE INDEX IF NOT EXISTS idx_activity_events_session ON activity_events(session_id);",
        )?;

        Ok(())
    }

    fn add_column_if_missing(
        &self,
        table: &str,
        column: &str,
        col_type: &str,
        default: &str,
    ) -> anyhow::Result<()> {
        let exists: bool = {
            let mut stmt = self
                .conn
                .prepare(&format!("PRAGMA table_info({})", table))?;
            let rows: Vec<String> = stmt
                .query_map([], |row| row.get::<_, String>(1))?
                .filter_map(|r| r.ok())
                .collect();
            rows.iter().any(|name| name == column)
        };
        if !exists {
            let sql = format!(
                "ALTER TABLE {} ADD COLUMN {} {} DEFAULT {}",
                table, column, col_type, default
            );
            self.conn.execute_batch(&sql)?;
            log::info!("Added column {}.{} ({})", table, column, col_type);
        }
        Ok(())
    }

    pub fn start_session(
        &self,
        class: &str,
        title: &str,
        workspace: &str,
        pid: Option<i32>,
    ) -> anyhow::Result<i64> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO sessions (class, title, workspace, started_at, activity_state, pid)
             VALUES (?1, ?2, ?3, ?4, 'active', ?5)",
            params![class, title, workspace, now, pid],
        )?;
        let id = self.conn.last_insert_rowid();
        if let Ok(mut starts) = self.monotonic_starts.lock() {
            starts.insert(id, Instant::now());
        }
        self.save_activity_event(id, "active")?;
        Ok(id)
    }

    /// Elapsed milliseconds since `session_id` was started.
    ///
    /// Uses the monotonic `Instant` recorded by `start_session` so clock jumps
    /// (e.g. dual-boot Windows/Linux RTC skew) cannot produce negative values.
    /// Falls back to wall-clock parsing for sessions started by a previous
    /// daemon process (rare), still clamped to >= 0.
    pub fn session_elapsed_ms(&self, session_id: i64) -> anyhow::Result<i64> {
        if let Ok(starts) = self.monotonic_starts.lock() {
            if let Some(started) = starts.get(&session_id) {
                let ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
                return Ok(ms.max(0));
            }
        }

        let started_at: String = self.conn.query_row(
            "SELECT started_at FROM sessions WHERE id = ?1",
            params![session_id],
            |row| row.get(0),
        )?;
        let started =
            chrono::DateTime::parse_from_rfc3339(&started_at)?.with_timezone(&chrono::Utc);
        Ok((chrono::Utc::now() - started).num_milliseconds().max(0))
    }

    pub fn end_session(&self, session_id: i64) -> anyhow::Result<()> {
        let now = chrono::Utc::now();
        let now_str = now.to_rfc3339();
        let date_str = now.format("%Y-%m-%d").to_string();

        let started_at: String = self.conn.query_row(
            "SELECT started_at FROM sessions WHERE id = ?1",
            params![session_id],
            |row| row.get(0),
        )?;

        let started: chrono::DateTime<chrono::Utc> =
            chrono::DateTime::parse_from_rfc3339(&started_at)?.with_timezone(&chrono::Utc);
        // Prefer the process-monotonic elapsed time: when the wall clock jumps
        // backwards the RFC3339 timestamps would yield a negative duration.
        let duration_ms = self.session_elapsed_ms(session_id).unwrap_or_else(|_| {
            // Fallback (should never be hit for an in-memory session): clamp a
            // wall-clock-negative duration to zero instead of storing negatives.
            (now - started).num_milliseconds().max(0)
        });

        // If the wall clock jumped while the session was running (NTP step,
        // dual-boot RTC skew), the recorded started_at no longer lines up with
        // the monotonic duration. Rewrite it from `now - duration` so the
        // stored timestamps stay ordered and the timeline remains usable.
        let wall_delta_ms = (now - started).num_milliseconds();
        let clock_jumped = (wall_delta_ms - duration_ms).abs() > 1000;
        let started = if clock_jumped {
            now - chrono::Duration::milliseconds(duration_ms)
        } else {
            started
        };

        let focused_ms = std::cmp::max(0, duration_ms - self.focused_threshold_ms);

        if clock_jumped {
            self.conn.execute(
                "UPDATE sessions SET ended_at = ?1, duration_ms = ?2, focused_ms = ?3, started_at = ?4 WHERE id = ?5",
                params![now_str, duration_ms, focused_ms, started.to_rfc3339(), session_id],
            )?;
        } else {
            self.conn.execute(
                "UPDATE sessions SET ended_at = ?1, duration_ms = ?2, focused_ms = ?3 WHERE id = ?4",
                params![now_str, duration_ms, focused_ms, session_id],
            )?;
        }
        if let Ok(mut starts) = self.monotonic_starts.lock() {
            starts.remove(&session_id);
        }

        let class: String = self.conn.query_row(
            "SELECT class FROM sessions WHERE id = ?1",
            params![session_id],
            |row| row.get(0),
        )?;

        let activity_state: String = self.conn.query_row(
            "SELECT activity_state FROM sessions WHERE id = ?1",
            params![session_id],
            |row| row.get(0),
        )?;
        let is_focused_session = if activity_state == "focused" {
            1i64
        } else {
            0i64
        };

        self.conn.execute(
            "INSERT INTO daily_summary (date, class, total_ms, session_count, focused_ms, focused_session_count)
             VALUES (?1, ?2, ?3, 1, ?4, ?5)
             ON CONFLICT(date, class) DO UPDATE SET
               total_ms = total_ms + ?3,
               session_count = session_count + 1,
               focused_ms = focused_ms + ?4,
               focused_session_count = focused_session_count + ?5",
             params![date_str, class, duration_ms, focused_ms, is_focused_session],
        )?;

        // Maintain hourly_summary incrementally so the server's fast path stays fresh.
        // Bucket by the session's LOCAL start hour (consistent with server's fallback).
        let local_hour = started.with_timezone(&chrono::Local).hour() as i64;
        if let Err(e) = self.conn.execute(
            "INSERT INTO hourly_summary (date, hour, class, total_ms, session_count, focused_ms)
             VALUES (?1, ?2, ?3, ?4, 1, ?5)
             ON CONFLICT(date, hour, class) DO UPDATE SET
               total_ms = total_ms + ?4,
               session_count = session_count + 1,
               focused_ms = focused_ms + ?5",
            params![date_str, local_hour, class, duration_ms, focused_ms],
        ) {
            log::warn!("Failed to upsert hourly_summary: {}", e);
        }

        self.close_activity_event(session_id);

        Ok(())
    }

    pub fn update_session_state(&self, session_id: i64, state: &str) -> anyhow::Result<()> {
        let prev_state: String = self.conn.query_row(
            "SELECT activity_state FROM sessions WHERE id = ?1",
            params![session_id],
            |row| row.get(0),
        )?;

        self.conn.execute(
            "UPDATE sessions SET activity_state = ?1 WHERE id = ?2",
            params![state, session_id],
        )?;

        self.close_activity_event(session_id);
        self.save_activity_event(session_id, state)?;

        log::info!("Session {} state: {} → {}", session_id, prev_state, state);
        Ok(())
    }

    fn save_activity_event(&self, session_id: i64, state: &str) -> anyhow::Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO activity_events (session_id, state, started_at) VALUES (?1, ?2, ?3)",
            params![session_id, state, now],
        )?;
        Ok(())
    }

    fn close_activity_event(&self, session_id: i64) {
        let now = chrono::Utc::now();
        let now_str = now.to_rfc3339();

        let started_at: Option<String> = self
            .conn
            .query_row(
                "SELECT started_at FROM activity_events
                 WHERE session_id = ?1 AND ended_at IS NULL
                 ORDER BY started_at DESC LIMIT 1",
                params![session_id],
                |row| row.get(0),
            )
            .ok();

        if let Some(ref sa) = started_at {
            if let Ok(sa_dt) = chrono::DateTime::parse_from_rfc3339(sa) {
                let sa_utc = sa_dt.with_timezone(&chrono::Utc);
                let dur = (now - sa_utc).num_milliseconds().max(0);
                let _ = self.conn.execute(
                    "UPDATE activity_events SET ended_at = ?1, duration_ms = ?2
                     WHERE session_id = ?3 AND ended_at IS NULL",
                    params![now_str, dur, session_id],
                );
            }
        }
    }

    pub fn update_ongoing_focused_ms(&self, session_id: i64) -> anyhow::Result<()> {
        let duration_ms = self.session_elapsed_ms(session_id)?;
        let focused_ms = std::cmp::max(0, duration_ms - self.focused_threshold_ms);

        self.conn.execute(
            "UPDATE sessions SET focused_ms = ?1 WHERE id = ?2",
            params![focused_ms, session_id],
        )?;
        Ok(())
    }

    pub fn current_session_id(&self) -> anyhow::Result<Option<i64>> {
        let result = self.conn.query_row(
            "SELECT id FROM sessions WHERE ended_at IS NULL ORDER BY started_at DESC LIMIT 1",
            [],
            |row| row.get(0),
        );
        match result {
            Ok(id) => Ok(Some(id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn current_session_state(&self) -> anyhow::Result<Option<(i64, String, i64)>> {
        let result = self.conn.query_row(
            "SELECT id, activity_state, COALESCE(focused_ms, 0) FROM sessions
             WHERE ended_at IS NULL ORDER BY started_at DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        );
        match result {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Current open session's (id, class, pid), for resource sampling.
    pub fn current_session_resources(&self) -> anyhow::Result<Option<(i64, String, i32)>> {
        let result = self.conn.query_row(
            "SELECT id, class, pid FROM sessions
             WHERE ended_at IS NULL AND pid IS NOT NULL
             ORDER BY started_at DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        );
        match result {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Record a CPU/memory sample for a session.
    pub fn save_resource_sample(
        &self,
        session_id: i64,
        class: &str,
        cpu_pct: f64,
        mem_kb: i64,
    ) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT INTO app_resources (session_id, class, sampled_at, cpu_pct, mem_kb)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                session_id,
                class,
                chrono::Utc::now().to_rfc3339(),
                cpu_pct,
                mem_kb
            ],
        )?;
        Ok(())
    }

    /// Record a desktop notification (an interruption).
    pub fn save_notification(&self, app: &str, summary: &str) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT INTO disruptions (kind, app, summary, occurred_at) VALUES ('notification', ?1, ?2, ?3)",
            params![app, summary, chrono::Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    /// Record a clipboard copy event.
    pub fn save_clipboard(&self) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT INTO disruptions (kind, occurred_at) VALUES ('clipboard', ?1)",
            params![chrono::Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

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

    pub fn set_goals(&self, goals: &[Goal]) -> anyhow::Result<()> {
        self.conn.execute("DELETE FROM goals", [])?;
        let mut stmt = self.conn.prepare(
            "INSERT INTO goals (name, target_type, target_key, daily_target_ms, enabled)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for g in goals {
            stmt.execute(params![
                g.name.trim(),
                g.target_type,
                g.target_key.as_deref(),
                g.daily_target_ms,
                if g.enabled { 1 } else { 0 },
            ])?;
        }
        Ok(())
    }

    /// Today's active milliseconds for a goal. "all" = total across classes.
    pub fn today_active_for_goal(&self, goal: &Goal) -> i64 {
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
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

    /// Update only the activity_state column, without touching activity_events.
    /// Used for retroactive state changes on already-ended sessions (e.g. idle → away).
    pub fn set_session_state_only(&self, session_id: i64, state: &str) -> anyhow::Result<()> {
        self.conn.execute(
            "UPDATE sessions SET activity_state = ?1 WHERE id = ?2",
            params![state, session_id],
        )?;
        Ok(())
    }

    pub fn end_current_session(&self) -> anyhow::Result<Option<i64>> {
        if let Some(id) = self.current_session_id()? {
            self.end_session(id)?;
            Ok(Some(id))
        } else {
            Ok(None)
        }
    }

    pub fn clear_orphaned_sessions(&self) -> anyhow::Result<usize> {
        // Delete activity_events FIRST: with PRAGMA foreign_keys=ON, deleting a
        // session that still has referencing events raises a constraint error,
        // which previously made this cleanup fail silently and left orphaned
        // sessions open until the first window event closed them with a bogus
        // multi-hour duration (see phantom "reboot" sessions).
        self.conn.execute(
            "DELETE FROM activity_events
             WHERE session_id IN (SELECT id FROM sessions WHERE ended_at IS NULL)",
            [],
        )?;
        let count = self
            .conn
            .execute("DELETE FROM sessions WHERE ended_at IS NULL", [])?;
        // All open sessions are gone; drop their monotonic start markers too.
        if let Ok(mut starts) = self.monotonic_starts.lock() {
            starts.clear();
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_fresh_and_upgrade_paths() {
        let dir = std::env::temp_dir().join(format!("hyprtrace-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");

        // Fresh DB: full migration
        let db = Database::open(&path, 20 * 60).unwrap();
        db.migrate().unwrap();

        // Verify new columns exist
        let cols: Vec<String> = db
            .conn
            .prepare("PRAGMA table_info(sessions)")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        assert!(cols.contains(&"activity_state".to_string()));
        assert!(cols.contains(&"focused_ms".to_string()));

        // Verify new tables exist
        let tables: Vec<String> = db
            .conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table'")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        assert!(tables.contains(&"activity_events".to_string()));
        assert!(tables.contains(&"hourly_summary".to_string()));

        // Running migrate again must be idempotent
        db.migrate().unwrap();

        // start → end session: focused_ms heuristic + hourly_summary upsert
        let id = db.start_session("code", "main.rs", "1", None).unwrap();
        db.end_session(id).unwrap();

        let (state, focused): (String, i64) = db
            .conn
            .query_row(
                "SELECT activity_state, focused_ms FROM sessions WHERE id = ?1",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(state, "active");
        assert_eq!(focused, 0); // short session → no focus time

        let hs_count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM hourly_summary", [], |r| r.get(0))
            .unwrap();
        assert_eq!(hs_count, 1);

        let ev_count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM activity_events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(ev_count, 1);

        drop(db);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn clear_orphaned_sessions_with_open_events() {
        let dir = std::env::temp_dir().join(format!("hyprtrace-test3-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");

        let db = Database::open(&path, 20 * 60).unwrap();
        db.migrate().unwrap();

        // Properly-ended session stays; orphans (with their open activity
        // events) get deleted — the FK constraint that previously made this
        // cleanup fail silently.
        let ended = db.start_session("kitty", "normal", "5", None).unwrap();
        db.end_session(ended).unwrap();
        db.start_session("kitty", "reboot", "5", None).unwrap();
        db.start_session("firefox", "tab", "2", None).unwrap();

        // At this point two sessions are open; clear them like startup does.
        let count = db.clear_orphaned_sessions().unwrap();
        assert_eq!(count, 2);

        let remaining: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining, 1); // only the properly-ended session remains

        let events_left: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM activity_events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(events_left, 1); // events of deleted orphans are gone too

        drop(db);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn migrate_v6_repairs_negative_durations() {
        let dir = std::env::temp_dir().join(format!("hyprtrace-test4-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");

        let db = Database::open(&path, 20 * 60).unwrap();
        db.migrate().unwrap();

        // Simulate a session whose duration was recorded before the clock-jump
        // fix, plus a polluted daily_summary and activity_event.
        db.conn
            .execute(
                "INSERT INTO sessions (class, title, workspace, started_at, ended_at, duration_ms, activity_state, focused_ms)
                 VALUES ('kitty', 'yay', '~', '2026-08-15T15:55:04.800+00:00', '2026-08-15T07:56:28.878+00:00', -28715922, 'active', 0)",
                [],
            )
            .unwrap();
        let sid = db.conn.last_insert_rowid();
        db.conn
            .execute(
                "INSERT INTO activity_events (session_id, state, started_at, ended_at, duration_ms)
                 VALUES (?1, 'active', '2026-08-15T15:55:04.801+00:00', '2026-08-15T07:54:59.690+00:00', -28805111)",
                params![sid],
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO daily_summary (date, class, total_ms, session_count, focused_ms, focused_session_count)
                 VALUES ('2026-08-15', 'kitty', -28715922, 1, 0, 0)",
                [],
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO hourly_summary (date, hour, class, total_ms, session_count, focused_ms)
                 VALUES ('2026-08-15', 23, 'kitty', -28715922, 1, 0)",
                [],
            )
            .unwrap();

        db.migrate_v6().unwrap();

        let (dur, focused): (i64, i64) = db
            .conn
            .query_row(
                "SELECT duration_ms, focused_ms FROM sessions WHERE id = ?1",
                params![sid],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(dur, 0);
        assert_eq!(focused, 0);

        let ev_dur: i64 = db
            .conn
            .query_row(
                "SELECT duration_ms FROM activity_events WHERE session_id = ?1",
                params![sid],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(ev_dur, 0);

        let daily: (i64, i64) = db
            .conn
            .query_row(
                "SELECT total_ms, session_count FROM daily_summary WHERE date = '2026-08-15' AND class = 'kitty'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(daily, (0, 1));

        let hourly: (i64, i64) = db
            .conn
            .query_row(
                "SELECT total_ms, session_count FROM hourly_summary WHERE date = '2026-08-15' AND hour = 23 AND class = 'kitty'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(hourly, (0, 1));

        drop(db);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn end_session_clamps_clock_going_backwards() {
        let dir = std::env::temp_dir().join(format!("hyprtrace-test5-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");

        let db = Database::open(&path, 20 * 60).unwrap();
        db.migrate().unwrap();

        // Session whose started_at is in the future: the wall clock was set
        // backwards while it was running. end_session must not store a negative
        // duration, and it should rewrite started_at so start <= end.
        let future = (chrono::Utc::now() + chrono::Duration::minutes(60)).to_rfc3339();
        db.conn
            .execute(
                "INSERT INTO sessions (class, title, workspace, started_at, activity_state, focused_ms)
                 VALUES ('kitty', 'yay', '~', ?1, 'active', 0)",
                params![future],
            )
            .unwrap();
        let sid = db.conn.last_insert_rowid();

        db.end_session(sid).unwrap();

        let (started_at, ended_at, duration_ms): (String, String, i64) = db
            .conn
            .query_row(
                "SELECT started_at, ended_at, duration_ms FROM sessions WHERE id = ?1",
                params![sid],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert!(duration_ms >= 0);
        let started = chrono::DateTime::parse_from_rfc3339(&started_at).unwrap();
        let ended = chrono::DateTime::parse_from_rfc3339(&ended_at).unwrap();
        assert!(ended >= started);

        let daily_negative: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM daily_summary WHERE total_ms < 0", [], |r| r.get(0))
            .unwrap();
        assert_eq!(daily_negative, 0);

        drop(db);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn state_transitions() {
        let dir = std::env::temp_dir().join(format!("hyprtrace-test2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");

        let db = Database::open(&path, 20 * 60).unwrap();
        db.migrate().unwrap();

        let id = db.start_session("firefox", "github", "2", None).unwrap();
        db.update_session_state(id, "focused").unwrap();

        let state: String = db
            .conn
            .query_row(
                "SELECT activity_state FROM sessions WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(state, "focused");

        // Two events: "active" from start, "focused" from transition; first is closed
        let (open, closed): (i64, i64) = db
            .conn
            .query_row(
                "SELECT SUM(CASE WHEN ended_at IS NULL THEN 1 ELSE 0 END),
                        SUM(CASE WHEN ended_at IS NOT NULL THEN 1 ELSE 0 END)
                 FROM activity_events",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(open, 1);
        assert_eq!(closed, 1);

        // Retroactive away marking on an ended session
        db.end_session(id).unwrap();
        db.set_session_state_only(id, "away").unwrap();
        let state: String = db
            .conn
            .query_row(
                "SELECT activity_state FROM sessions WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(state, "away");

        drop(db);
        std::fs::remove_dir_all(&dir).ok();
    }
}
