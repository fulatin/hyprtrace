use anyhow::Context;
use rusqlite::{params, Connection};
use std::path::Path;

pub struct Database {
    conn: Connection,
}

impl Database {
    /// Open SQLite database with WAL mode and busy_timeout
    pub fn open(path: &Path) -> anyhow::Result<Self> {
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

        Ok(Self { conn })
    }

    /// Create tables and indexes
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
        Ok(())
    }

    /// Start a new window session, return session id
    pub fn start_session(&self, class: &str, title: &str, workspace: &str) -> anyhow::Result<i64> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO sessions (class, title, workspace, started_at) VALUES (?1, ?2, ?3, ?4)",
            params![class, title, workspace, now],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// End session, update ended_at and duration_ms, upsert daily_summary
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

        self.conn.execute(
            "UPDATE sessions SET ended_at = ?1, duration_ms = ?2 WHERE id = ?3",
            params![now_str, duration_ms, session_id],
        )?;

        let class: String = self.conn.query_row(
            "SELECT class FROM sessions WHERE id = ?1",
            params![session_id],
            |row| row.get(0),
        )?;

        self.conn.execute(
            "INSERT INTO daily_summary (date, class, total_ms, session_count)
             VALUES (?1, ?2, ?3, 1)
             ON CONFLICT(date, class) DO UPDATE SET
               total_ms = total_ms + ?3,
               session_count = session_count + 1",
            params![date_str, class, duration_ms],
        )?;

        Ok(())
    }

    /// Get the current active session id (if any)
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

    /// End the current active session if one exists, return its id
    pub fn end_current_session(&self) -> anyhow::Result<Option<i64>> {
        if let Some(id) = self.current_session_id()? {
            self.end_session(id)?;
            Ok(Some(id))
        } else {
            Ok(None)
        }
    }

    /// Delete orphaned sessions (ended_at IS NULL) left from previous crashes/shutdowns.
    /// These sessions cannot have accurate duration computed, so delete them
    /// instead of calling end_session() which would compute a bogus multi-hour duration.
    pub fn clear_orphaned_sessions(&self) -> anyhow::Result<usize> {
        let count = self.conn.execute(
            "DELETE FROM sessions WHERE ended_at IS NULL",
            [],
        )?;
        Ok(count)
    }
}