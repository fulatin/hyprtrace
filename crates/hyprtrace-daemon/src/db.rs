use anyhow::Context;
use chrono::Timelike;
use rusqlite::{params, Connection};
use std::path::Path;

pub struct Database {
    conn: Connection,
    focused_threshold_ms: i64,
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
    ) -> anyhow::Result<i64> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO sessions (class, title, workspace, started_at, activity_state)
             VALUES (?1, ?2, ?3, ?4, 'active')",
            params![class, title, workspace, now],
        )?;
        let id = self.conn.last_insert_rowid();
        self.save_activity_event(id, "active")?;
        Ok(id)
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
        let duration_ms = (now - started).num_milliseconds();

        let focused_ms = std::cmp::max(0, duration_ms - self.focused_threshold_ms);

        self.conn.execute(
            "UPDATE sessions SET ended_at = ?1, duration_ms = ?2, focused_ms = ?3 WHERE id = ?4",
            params![now_str, duration_ms, focused_ms, session_id],
        )?;

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
        let is_focused_session = if activity_state == "focused" { 1i64 } else { 0i64 };

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

    pub fn update_session_state(
        &self,
        session_id: i64,
        state: &str,
    ) -> anyhow::Result<()> {
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
                let dur = (now - sa_utc).num_milliseconds();
                let _ = self.conn.execute(
                    "UPDATE activity_events SET ended_at = ?1, duration_ms = ?2
                     WHERE session_id = ?3 AND ended_at IS NULL",
                    params![now_str, dur, session_id],
                );
            }
        }
    }

    pub fn update_ongoing_focused_ms(&self, session_id: i64) -> anyhow::Result<()> {
        let started_at: String = self.conn.query_row(
            "SELECT started_at FROM sessions WHERE id = ?1",
            params![session_id],
            |row| row.get(0),
        )?;
        let started =
            chrono::DateTime::parse_from_rfc3339(&started_at)?.with_timezone(&chrono::Utc);
        let duration_ms = (chrono::Utc::now() - started).num_milliseconds();
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

    pub fn get_session_started_at(&self, session_id: i64) -> anyhow::Result<String> {
        self.conn
            .query_row(
                "SELECT started_at FROM sessions WHERE id = ?1",
                params![session_id],
                |row| row.get(0),
            )
            .map_err(Into::into)
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
        let count = self.conn.execute("DELETE FROM sessions WHERE ended_at IS NULL", [])?;
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
        let id = db.start_session("code", "main.rs", "1").unwrap();
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
        let ended = db.start_session("kitty", "normal", "5").unwrap();
        db.end_session(ended).unwrap();
        db.start_session("kitty", "reboot", "5").unwrap();
        db.start_session("firefox", "tab", "2").unwrap();

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
    fn state_transitions() {
        let dir = std::env::temp_dir().join(format!("hyprtrace-test2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");

        let db = Database::open(&path, 20 * 60).unwrap();
        db.migrate().unwrap();

        let id = db.start_session("firefox", "github", "2").unwrap();
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
