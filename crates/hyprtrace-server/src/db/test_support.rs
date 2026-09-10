use rusqlite::{params, Connection};

use super::Database;

    /// Open a throwaway in-memory DB and create the tables the delete paths
    /// touch (mirroring the daemon's schema subset we care about here).
    pub(crate) fn test_db() -> Database {
        let db = Database {
            conn: Connection::open_in_memory().unwrap(),
            focused_threshold_ms: 20 * 60 * 1000,
        };
        db.conn
            .execute_batch(
                "CREATE TABLE sessions (
                    id          INTEGER PRIMARY KEY AUTOINCREMENT,
                    class       TEXT NOT NULL,
                    title       TEXT NOT NULL DEFAULT '',
                    workspace   TEXT,
                    started_at  TEXT NOT NULL,
                    ended_at    TEXT,
                    duration_ms INTEGER DEFAULT 0,
                    activity_state TEXT,
                    focused_ms  INTEGER,
                    date_local  TEXT DEFAULT ''
                );
                CREATE TABLE app_resources (
                    id         INTEGER PRIMARY KEY AUTOINCREMENT,
                    session_id INTEGER,
                    class      TEXT NOT NULL,
                    sampled_at TEXT NOT NULL,
                    cpu_pct    REAL,
                    mem_kb     INTEGER,
                    date_local TEXT DEFAULT '',
                    FOREIGN KEY (session_id) REFERENCES sessions(id)
                );
                CREATE TABLE activity_events (
                    id          INTEGER PRIMARY KEY AUTOINCREMENT,
                    session_id  INTEGER,
                    state       TEXT NOT NULL,
                    started_at  TEXT NOT NULL,
                    ended_at    TEXT,
                    duration_ms INTEGER DEFAULT 0,
                    date_local  TEXT DEFAULT '',
                    FOREIGN KEY (session_id) REFERENCES sessions(id)
                );
                CREATE TABLE daily_summary (
                    id            INTEGER PRIMARY KEY AUTOINCREMENT,
                    date          TEXT NOT NULL,
                    class         TEXT NOT NULL,
                    total_ms      INTEGER NOT NULL DEFAULT 0,
                    session_count INTEGER NOT NULL DEFAULT 0,
                    focused_ms    INTEGER NOT NULL DEFAULT 0,
                    focused_session_count INTEGER NOT NULL DEFAULT 0,
                    UNIQUE(date, class)
                );
                CREATE TABLE hourly_summary (
                    id            INTEGER PRIMARY KEY AUTOINCREMENT,
                    date          TEXT NOT NULL,
                    hour          INTEGER NOT NULL,
                    class         TEXT NOT NULL,
                    total_ms      INTEGER NOT NULL DEFAULT 0,
                    session_count INTEGER NOT NULL DEFAULT 0,
                    focused_ms    INTEGER NOT NULL DEFAULT 0,
                    UNIQUE(date, hour, class)
                );
                PRAGMA foreign_keys = ON;",
            )
            .unwrap();
        db
    }
    pub(crate) fn insert_session(db: &Database, class: &str, started_at: &str, ended_at: Option<&str>) -> i64 {
        // date_local is set to the date portion of started_at so the tests are
        // deterministic regardless of the host's timezone.
        let date_local = started_at.get(..10).unwrap_or("").to_string();
        db.conn
            .execute(
                "INSERT INTO sessions (class, title, started_at, ended_at, duration_ms, date_local)
                 VALUES (?1, '', ?2, ?3, 1000, ?4)",
                params![class, started_at, ended_at, date_local],
            )
            .unwrap();
        db.conn.last_insert_rowid()
    }
    pub(crate) fn project_test_db() -> Database {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "PRAGMA foreign_keys=ON;
             CREATE TABLE IF NOT EXISTS projects (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                name       TEXT NOT NULL UNIQUE,
                color      TEXT NOT NULL DEFAULT '#22d3ee',
                sort_order INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
             );
             CREATE TABLE IF NOT EXISTS project_rules (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                pattern    TEXT NOT NULL,
                priority   INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE IF NOT EXISTS sessions (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                class        TEXT NOT NULL,
                title        TEXT NOT NULL DEFAULT '',
                workspace    TEXT,
                started_at   TEXT NOT NULL,
                ended_at     TEXT,
                duration_ms  INTEGER,
                activity_state TEXT,
                focused_ms   INTEGER,
                date_local   TEXT DEFAULT ''
             );",
        )
        .unwrap();
        Database {
            conn,
            focused_threshold_ms: 30_000,
        }
    }
    pub(crate) fn tracked_session(conn: &Connection, class: &str, ms: i64) {
        conn.execute(
            "INSERT INTO sessions (class, title, started_at, ended_at, duration_ms, date_local)
             VALUES (?1, '', '2025-01-01T00:00:00Z', '2025-01-01T01:00:00Z', ?2, '2025-01-01')",
            params![class, ms],
        )
        .unwrap();
    }
