use rusqlite::{params, Connection};

use super::Database;

    /// Open a throwaway in-memory DB carrying the CANONICAL schema.
    ///
    /// This used to hand-write the handful of tables the tests touch, which is
    /// exactly how a fixture drifts away from production: `sessions` was already
    /// declared twice, with two different column definitions, and nothing would
    /// have caught it. The fixture now runs the same `schema/hyprtrace.sql` that
    /// `Database::migrate()` runs, with the same `foreign_keys=ON` that
    /// `Database::open()` sets, so a test can no longer exercise a schema the
    /// real server never sees.
    pub(crate) fn test_db() -> Database {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        conn.execute_batch(crate::schema::SCHEMA_SQL).unwrap();
        Database {
            conn,
            focused_threshold_ms: 20 * 60 * 1000,
        }
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
    /// `test_db` plus the server-owned tables, created by the same functions the
    /// server calls at startup (`app_categories`, `projects`, `project_rules`).
    pub(crate) fn project_test_db() -> Database {
        let db = test_db();
        db.ensure_projects().unwrap();
        Database {
            conn: db.conn,
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
