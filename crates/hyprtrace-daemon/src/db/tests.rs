    use super::*;
    use chrono::Timelike;
    use rusqlite::params;

    /// Stable textual dump of every table column and index in `conn`, in the
    /// exact format of `schema/expected-columns.txt`.
    fn schema_fingerprint(conn: &Connection) -> String {
        let names: Vec<(String, String)> = conn
            .prepare("SELECT type, name FROM sqlite_master WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();

        let mut out = String::new();
        for (kind, name) in &names {
            if kind == "index" {
                out.push_str(&format!("IDX|{}\n", name));
            } else if kind == "table" {
                let cols: Vec<(String, String, i64, Option<String>)> = conn
                    .prepare(&format!("PRAGMA table_info({})", name))
                    .unwrap()
                    .query_map([], |r| Ok((r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
                    .unwrap()
                    .map(|r| r.unwrap())
                    .collect();
                for (col, ty, notnull, dflt) in cols {
                    out.push_str(&format!(
                        "COL|{}|{}|{}|{}|{}\n",
                        name,
                        col,
                        ty,
                        notnull,
                        dflt.unwrap_or_else(|| "<null>".to_string())
                    ));
                }
            }
        }
        out.trim_end().to_string()
    }

    fn temp_db(tag: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("hyprtrace-schema-{}-{}", tag, std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");
        (dir, path)
    }

    /// The DDL both crates share (`schema/hyprtrace.sql`) has to produce exactly
    /// the committed snapshot. This is what keeps the daemon and the server from
    /// drifting apart the way the old two-hand-written-copies scheme did.
    #[test]
    fn migrate_fresh_db_matches_the_canonical_schema() {
        let (dir, path) = temp_db("fresh");
        let db = Database::open(&path, 20 * 60).unwrap();
        db.migrate().unwrap();
        assert_eq!(
            schema_fingerprint(&db.conn),
            crate::schema::EXPECTED_SCHEMA.trim_end()
        );
        drop(db);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A database left behind by an old build (v1 tables only, none of the
    /// columns v2/v3/v7 added) must end up with the very same shape as a fresh
    /// one: the shared DDL creates whatever is missing and the legacy ALTERs
    /// upgrade the tables that already exist.
    #[test]
    fn migrate_upgrades_a_legacy_database_to_the_canonical_schema() {
        let (dir, path) = temp_db("legacy");
        let db = Database::open(&path, 20 * 60).unwrap();
        db.conn
            .execute_batch(
                "CREATE TABLE sessions (
                    id          INTEGER PRIMARY KEY AUTOINCREMENT,
                    class       TEXT NOT NULL,
                    title       TEXT NOT NULL DEFAULT '',
                    workspace   TEXT,
                    started_at  TEXT NOT NULL,
                    ended_at    TEXT,
                    duration_ms INTEGER DEFAULT 0
                );

                CREATE TABLE daily_summary (
                    id            INTEGER PRIMARY KEY AUTOINCREMENT,
                    date          TEXT NOT NULL,
                    class         TEXT NOT NULL,
                    total_ms      INTEGER NOT NULL DEFAULT 0,
                    session_count INTEGER NOT NULL DEFAULT 0,
                    UNIQUE(date, class)
                );

                CREATE TABLE ai_conversations (
                    id         INTEGER PRIMARY KEY AUTOINCREMENT,
                    created_at TEXT NOT NULL,
                    role       TEXT NOT NULL,
                    content    TEXT NOT NULL,
                    model      TEXT NOT NULL
                );",
            )
            .unwrap();

        db.migrate().unwrap();

        assert_eq!(
            schema_fingerprint(&db.conn),
            crate::schema::EXPECTED_SCHEMA.trim_end()
        );
        drop(db);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// `migrate()` runs the whole file on every start, so a second run must be
    /// a no-op.
    #[test]
    fn migrate_is_idempotent() {
        let (dir, path) = temp_db("idempotent");
        let db = Database::open(&path, 20 * 60).unwrap();
        db.migrate().unwrap();
        let first = schema_fingerprint(&db.conn);
        db.migrate().unwrap();
        assert_eq!(schema_fingerprint(&db.conn), first);
        drop(db);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn today_active_for_goal_reads_local_today_bucket() {
        let dir = std::env::temp_dir().join(format!("hyprtrace-test-goal-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");

        let db = Database::open(&path, 20 * 60).unwrap();
        db.migrate().unwrap();

        // A row bucketed on today's LOCAL date must be visible to goal
        // progress (writer and reader share the same bucket key).
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        db.conn
            .execute(
                "INSERT INTO daily_summary (date, class, total_ms, session_count) VALUES (?1, 'measured', 1234, 1)",
                params![today],
            )
            .unwrap();

        let goal = Goal {
            id: None,
            name: "g".to_string(),
            target_type: "class".to_string(),
            target_key: Some("measured".to_string()),
            daily_target_ms: 3_600_000,
            enabled: true,
        };
        assert_eq!(db.today_active_for_goal(&goal), 1234);

        drop(db);
        std::fs::remove_dir_all(&dir).ok();
    }

    // ---- M7: transaction atomicity (failure injection via triggers) ----

    #[test]
    fn end_session_is_atomic_on_injected_failure() {
        let dir = std::env::temp_dir().join(format!("hyprtrace-test-m7b-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");

        let db = Database::open(&path, 20 * 60).unwrap();
        db.migrate().unwrap();

        let id = db.start_session("boomclass", "title", "1", None).unwrap();

        // Inject a failure after the sessions UPDATE: the trigger aborts the
        // daily_summary INSERT for this class.
        db.conn
            .execute_batch(
                "CREATE TRIGGER fail_boom_daily BEFORE INSERT ON daily_summary
                 WHEN NEW.class = 'boomclass'
                 BEGIN SELECT RAISE(ABORT, 'injected failure'); END;",
            )
            .unwrap();

        assert!(
            db.end_session(id).is_err(),
            "injected trigger must fail the end"
        );

        // Atomic: the session must remain open (no half-written end)...
        let ended_at: Option<String> = db
            .conn
            .query_row(
                "SELECT ended_at FROM sessions WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(ended_at.is_none(), "session row must be rolled back");

        // ...and no summary rows may leak from the aborted attempt.
        let ds: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM daily_summary", [], |r| r.get(0))
            .unwrap();
        assert_eq!(ds, 0);
        let hs: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM hourly_summary", [], |r| r.get(0))
            .unwrap();
        assert_eq!(hs, 0);

        drop(db);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn end_session_commits_every_aggregate() {
        let dir = std::env::temp_dir().join(format!("hyprtrace-test-m7c-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");

        let db = Database::open(&path, 20 * 60).unwrap();
        db.migrate().unwrap();

        // Happy path still commits everything.
        let id = db.start_session("code", "main.rs", "1", None).unwrap();
        db.end_session(id).unwrap();
        let ended_at: String = db
            .conn
            .query_row(
                "SELECT ended_at FROM sessions WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(!ended_at.is_empty());
        let (ds, hs): (i64, i64) = db
            .conn
            .query_row(
                "SELECT (SELECT COUNT(*) FROM daily_summary),
                        (SELECT COUNT(*) FROM hourly_summary)",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((ds, hs), (1, 1));

        drop(db);
        std::fs::remove_dir_all(&dir).ok();
    }

    // ---- pre-existing coverage ----

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
    fn finalize_orphaned_sessions_keeps_records() {
        let dir = std::env::temp_dir().join(format!("hyprtrace-test3-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");

        let db = Database::open(&path, 20 * 60).unwrap();
        db.migrate().unwrap();

        // One properly ended session plus two left open by a crash. The orphans
        // must survive startup cleanup (finalised, not deleted).
        let ended = db.start_session("kitty", "normal", "5", None).unwrap();
        db.end_session(ended).unwrap();
        let orphan_a = db.start_session("kitty", "reboot", "5", None).unwrap();
        let orphan_b = db.start_session("firefox", "tab", "2", None).unwrap();

        // Give the orphans some activity so the end time can be derived from it.
        for sid in [orphan_a, orphan_b] {
            db.conn
                .execute(
                    "INSERT INTO activity_events (session_id, state, started_at, ended_at, duration_ms)
                     VALUES (?1, 'active', ?2, ?2, 0)",
                    params![sid, chrono::Utc::now().to_rfc3339()],
                )
                .unwrap();
        }

        let count = db.finalize_orphaned_sessions().unwrap();
        assert_eq!(count, 2, "both open sessions should be finalised");

        let remaining: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining, 3, "orphans are kept, not deleted");

        let still_open: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM sessions WHERE ended_at IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(still_open, 0, "no session stays open");

        // End time must not precede the start time, and duration must be >= 0.
        let bad: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM sessions WHERE ended_at < started_at OR duration_ms < 0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(bad, 0, "finalised timestamps stay ordered");

        let events_left: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM activity_events", [], |r| r.get(0))
            .unwrap();
        assert!(events_left >= 1, "events of finalised sessions are kept");

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
        let started_ts = "2026-08-15T15:55:04.800+00:00";
        let started_dt = chrono::DateTime::parse_from_rfc3339(started_ts).unwrap();
        let local_dt = started_dt.with_timezone(&chrono::Local);
        let expected_local_date = local_dt.format("%Y-%m-%d").to_string();
        let expected_local_hour = local_dt.hour() as i64;
        db.conn
            .execute(
                "INSERT INTO sessions (class, title, workspace, started_at, ended_at, duration_ms, activity_state, focused_ms)
                 VALUES ('kitty', 'yay', '~', ?1, '2026-08-15T07:56:28.878+00:00', -28715922, 'active', 0)",
                params![started_ts],
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
                "SELECT total_ms, session_count FROM daily_summary WHERE date = ?1 AND class = 'kitty'",
                params![expected_local_date],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(daily, (0, 1));

        let hourly: (i64, i64) = db
            .conn
            .query_row(
                "SELECT total_ms, session_count FROM hourly_summary WHERE date = ?1 AND hour = ?2 AND class = 'kitty'",
                params![expected_local_date, expected_local_hour],
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
            .query_row(
                "SELECT COUNT(*) FROM daily_summary WHERE total_ms < 0",
                [],
                |r| r.get(0),
            )
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

    #[test]
    fn end_session_updates_summaries_and_resources() {
        let dir = std::env::temp_dir().join(format!("hyprtrace-test6-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");

        let db = Database::open(&path, 1).unwrap();
        db.migrate().unwrap();

        let id = db.start_session("code", "main.rs", "1", None).unwrap();
        db.save_resource_sample(id, "code", 12.5, 2048).unwrap();
        db.update_session_state(id, "focused").unwrap();
        db.end_session(id).unwrap();

        // Session duration/focus are never negative.
        let (duration_ms, focused_ms): (i64, i64) = db
            .conn
            .query_row(
                "SELECT duration_ms, focused_ms FROM sessions WHERE id = ?1",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert!(duration_ms >= 0);
        assert!(focused_ms >= 0);

        // Daily and hourly summaries are updated incrementally.
        let daily: (i64, i64) = db
            .conn
            .query_row(
                "SELECT COUNT(*), COALESCE(SUM(session_count), 0) FROM daily_summary",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(daily, (1, 1));

        let hourly: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM hourly_summary", [], |r| r.get(0))
            .unwrap();
        assert_eq!(hourly, 1);

        // All activity events are closed once the session ends.
        let open_events: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM activity_events WHERE ended_at IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(open_events, 0);
        let total_events: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM activity_events", [], |r| r.get(0))
            .unwrap();
        assert!(total_events >= 1);

        // The resource sample was persisted against the session.
        let resource_rows: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM app_resources WHERE session_id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(resource_rows >= 1);

        drop(db);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn end_session_buckets_by_start_date() {
        let dir = std::env::temp_dir().join(format!("hyprtrace-test7-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");

        let db = Database::open(&path, 20 * 60).unwrap();
        db.migrate().unwrap();

        // Simulate a session started just before LOCAL midnight. It must be
        // bucketed on the LOCAL day it started, matching the daemon's
        // date_local field and the frontend's local-date queries.
        let started_at_ts = "2026-08-15T23:50:00+00:00";
        let expected_local_date = chrono::DateTime::parse_from_rfc3339(started_at_ts)
            .unwrap()
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d")
            .to_string();
        db.conn
            .execute(
                "INSERT INTO sessions (class, title, workspace, started_at, activity_state, focused_ms)
                 VALUES ('kitty', 'late', '1', ?1, 'active', 0)",
                params![started_at_ts],
            )
            .unwrap();
        let id = db.conn.last_insert_rowid();

        db.end_session(id).unwrap();

        let (daily_date, daily_total): (String, i64) = db
            .conn
            .query_row(
                "SELECT date, total_ms FROM daily_summary WHERE class = 'kitty'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(daily_date, expected_local_date);
        assert!(daily_total >= 0);

        let hourly_date: String = db
            .conn
            .query_row(
                "SELECT date FROM hourly_summary WHERE class = 'kitty'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hourly_date, expected_local_date);

        drop(db);
        std::fs::remove_dir_all(&dir).ok();
    }
