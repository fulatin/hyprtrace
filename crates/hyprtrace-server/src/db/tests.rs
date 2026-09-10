    use super::*;
    use rusqlite::params;
    use crate::models::{Goal, Project, ProjectRule};
    use super::like::like_match;
    use super::test_support::{insert_session, project_test_db, test_db, tracked_session};

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

    /// The server must produce exactly the same schema as the daemon: both embed
    /// `schema/hyprtrace.sql` and assert against the same committed snapshot, so
    /// a change that only one side picks up fails the other side's tests. The
    /// server-only tables (`app_categories`, `projects`, `project_rules`) are
    /// created by `ensure_*`, which this test deliberately does not call.
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
    fn like_match_wildcards() {
        assert!(like_match("firefox", "firefox"));
        assert!(like_match("fire%", "firefox"));
        assert!(like_match("%fox", "firefox"));
        assert!(like_match("firefox%", "firefox"));
        assert!(like_match("f_x", "fox"));
        assert!(like_match("%", ""));
        assert!(like_match("", ""));
        // Case-insensitive.
        assert!(like_match("Fire%", "firefox"));

        assert!(!like_match("firefox", "firefox-esr"));
        assert!(!like_match("firefox", "chromium"));
        // `_` is exactly one character, not zero or two.
        assert!(!like_match("f_x", "fx"));
        assert!(!like_match("f_x", "foox"));
        // No backtracking into a dead end.
        assert!(!like_match("a%b%c", "abb"));
    }

    #[test]
    fn like_match_question_mark_is_literal() {
        // SQLite's LIKE has no `?` operator. Treating it as a single-char
        // wildcard made a rule like "Code?" also match "Coder".
        assert!(like_match("code?", "code?"));
        assert!(!like_match("code?", "coder"));
        assert!(!like_match("code?", "code"));
    }

    #[test]
    fn like_match_backslash_escapes_wildcards() {
        assert!(like_match("100\\%", "100%"));
        assert!(!like_match("100\\%", "100x"));
        assert!(like_match("a\\_b", "a_b"));
        assert!(!like_match("a\\_b", "axb"));
        assert!(like_match("a\\\\b", "a\\b"));
        // A trailing lone backslash is a literal backslash.
        assert!(like_match("a\\", "a\\"));
    }

    #[test]
    fn set_goals_merges_by_scope_instead_of_wiping() {
        let db = Database::open(std::path::Path::new(":memory:"), 60).unwrap();
        db.migrate().unwrap();
        let existing = vec![
            Goal {
                id: None,
                name: "total".into(),
                target_type: "all".into(),
                target_key: None,
                daily_target_ms: 3_600_000,
                enabled: true,
            },
            Goal {
                id: None,
                name: "browsing".into(),
                target_type: "class".into(),
                target_key: Some("firefox".into()),
                daily_target_ms: 1_800_000,
                enabled: true,
            },
        ];
        db.set_goals(&existing, true).unwrap();

        // One goal, no mention of the other one.
        db.set_goals(
            &[Goal {
                id: None,
                name: "screen time".into(),
                target_type: "all".into(),
                target_key: None,
                daily_target_ms: 7_200_000,
                enabled: false,
            }],
            false,
        )
        .unwrap();

        let goals = db.goals().unwrap();
        assert_eq!(goals.len(), 2, "the unrelated goal must survive: {goals:?}");
        let total = goals
            .iter()
            .find(|g| g.target_type == "all")
            .expect("the 'all' goal should have been updated, not duplicated");
        assert_eq!(total.name, "screen time");
        assert_eq!(total.daily_target_ms, 7_200_000);
        assert!(!total.enabled);
        // The firefox goal is untouched.
        assert!(goals
            .iter()
            .any(|g| g.name == "browsing" && g.daily_target_ms == 1_800_000 && g.enabled));
    }

    #[test]
    fn set_goals_matches_scope_regardless_of_empty_or_null_key() {
        let db = Database::open(std::path::Path::new(":memory:"), 60).unwrap();
        db.migrate().unwrap();
        let goal = |key: Option<&str>| Goal {
            id: None,
            name: "g".into(),
            target_type: "all".into(),
            target_key: key.map(String::from),
            daily_target_ms: 1,
            enabled: true,
        };
        // Stored with a NULL key, updated with an empty string (and back).
        db.set_goals(&[goal(None)], true).unwrap();
        db.set_goals(&[goal(Some(""))], false).unwrap();
        assert_eq!(
            db.goals().unwrap().len(),
            1,
            "empty key must not insert a duplicate"
        );
        db.set_goals(&[goal(Some("  "))], false).unwrap();
        assert_eq!(db.goals().unwrap().len(), 1, "whitespace key is also empty");
    }

    #[test]
    fn set_goals_replace_all_still_replaces() {
        let db = Database::open(std::path::Path::new(":memory:"), 60).unwrap();
        db.migrate().unwrap();
        db.set_goals(
            &[
                Goal {
                    id: None,
                    name: "a".into(),
                    target_type: "all".into(),
                    target_key: None,
                    daily_target_ms: 1,
                    enabled: true,
                },
                Goal {
                    id: None,
                    name: "b".into(),
                    target_type: "class".into(),
                    target_key: Some("kitty".into()),
                    daily_target_ms: 1,
                    enabled: true,
                },
            ],
            true,
        )
        .unwrap();
        assert_eq!(db.goals().unwrap().len(), 2);

        db.set_goals(
            &[Goal {
                id: None,
                name: "c".into(),
                target_type: "class".into(),
                target_key: Some("alacritty".into()),
                daily_target_ms: 1,
                enabled: true,
            }],
            true,
        )
        .unwrap();
        let goals = db.goals().unwrap();
        assert_eq!(goals.len(), 1);
        assert_eq!(goals[0].name, "c");
    }

    #[test]
    fn delete_goal_removes_only_the_named_scope() {
        let db = Database::open(std::path::Path::new(":memory:"), 60).unwrap();
        db.migrate().unwrap();
        db.set_goals(
            &[
                Goal {
                    id: None,
                    name: "a".into(),
                    target_type: "all".into(),
                    target_key: None,
                    daily_target_ms: 1,
                    enabled: true,
                },
                Goal {
                    id: None,
                    name: "b".into(),
                    target_type: "class".into(),
                    target_key: Some("kitty".into()),
                    daily_target_ms: 1,
                    enabled: true,
                },
            ],
            true,
        )
        .unwrap();

        let removed = db.delete_goal(None, Some("class"), Some("kitty")).unwrap();
        assert_eq!(removed, 1);
        let goals = db.goals().unwrap();
        assert_eq!(goals.len(), 1);
        assert_eq!(goals[0].name, "a");

        // Deleting something that isn't there is a no-op, not an error.
        assert_eq!(
            db.delete_goal(None, Some("class"), Some("nope")).unwrap(),
            0
        );
        assert_eq!(db.goals().unwrap().len(), 1);
    }

    #[test]
    fn migrate_fresh_db_creates_schema_and_date_local() {
        let db = Database::open(std::path::Path::new(":memory:"), 60).unwrap();
        db.migrate().unwrap();
        db.migrate().unwrap(); // idempotent

        let tables: Vec<String> = db
            .conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table'")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        for expected in [
            "sessions",
            "daily_summary",
            "ai_conversations",
            "activity_events",
            "hourly_summary",
            "app_resources",
            "disruptions",
            "goals",
        ] {
            assert!(
                tables.contains(&expected.to_string()),
                "missing table {expected}"
            );
        }

        // date_local must exist on the date-keyed tables (v7) so the server's
        // date-local queries work even when it initializes the DB first.
        for table in [
            "sessions",
            "disruptions",
            "activity_events",
            "app_resources",
        ] {
            let cols: Vec<String> = db
                .conn
                .prepare(&format!("PRAGMA table_info({table})"))
                .unwrap()
                .query_map([], |r| r.get::<_, String>(1))
                .unwrap()
                .filter_map(|r| r.ok())
                .collect();
            assert!(
                cols.contains(&"date_local".to_string()),
                "{table} missing date_local"
            );
        }

        // Queries the API handlers run must not fail on a fresh DB.
        db.today_summary("2026-09-05").unwrap();
        db.goals().unwrap();
        db.current_status().unwrap();
    }

    #[test]
    fn migrate_v6_repairs_negative_durations() {
        let db = Database::open(std::path::Path::new(":memory:"), 60).unwrap();
        db.migrate().unwrap();

        // A wall-clock jump left a negative duration and a polluted summary.
        db.conn
            .execute(
                "INSERT INTO sessions (class, title, started_at, ended_at, duration_ms, activity_state, focused_ms)
                 VALUES ('kitty', 'x', '2026-09-05T10:00:00+00:00', '2026-09-05T10:05:00+00:00', -300000, 'active', -100)",
                [],
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO daily_summary (date, class, total_ms, session_count, focused_ms)
                 VALUES ('2026-09-05', 'kitty', -300000, 1, -100)",
                [],
            )
            .unwrap();

        // Re-running migrate() triggers the v6 repair: negative durations are
        // clamped and the summaries are rebuilt from the (now non-negative)
        // sessions.
        db.migrate().unwrap();

        let session_negative: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM sessions WHERE duration_ms < 0 OR focused_ms < 0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(session_negative, 0, "session rows must be clamped");

        let summary_negative: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM daily_summary WHERE total_ms < 0 OR focused_ms < 0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(summary_negative, 0, "summaries must be rebuilt, not negative");
    }

    #[test]
    fn report_and_ranking_smoke() {
        let dir =
            std::env::temp_dir().join(format!("hyprtrace-server-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");

        // Server Database::open does not create the schema; build only the
        // minimal tables this test needs (no daemon migration here).
        let db = Database::open(&path, 1).unwrap();
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
                    activity_state TEXT DEFAULT 'active',
                    focused_ms  INTEGER DEFAULT 0,
                    date_local  TEXT DEFAULT ''
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

                CREATE TABLE disruptions (
                    id          INTEGER PRIMARY KEY AUTOINCREMENT,
                    kind        TEXT NOT NULL,
                    app         TEXT,
                    summary     TEXT,
                    occurred_at TEXT NOT NULL,
                    date_local  TEXT DEFAULT ''
                );

                CREATE TABLE app_categories (
                    id       INTEGER PRIMARY KEY AUTOINCREMENT,
                    pattern  TEXT NOT NULL UNIQUE,
                    category TEXT NOT NULL,
                    priority INTEGER NOT NULL DEFAULT 0
                );",
            )
            .unwrap();

        // Two ended sessions for the same class on the same day.
        let from = "2026-01-15";
        let to = "2026-01-15";
        for i in 0..2 {
            db.conn
                .execute(
                    "INSERT INTO sessions (class, title, started_at, ended_at, duration_ms, focused_ms)
                     VALUES ('code', ?1, ?2, ?2, 60000, 30000)",
                    params![format!("main{}.rs", i), "2026-01-15T09:30:00+00:00"],
                )
                .unwrap();
        }

        // One matching daily_summary row drives app_ranking and report.
        db.conn
            .execute(
                "INSERT INTO daily_summary (date, class, total_ms, session_count, focused_ms, focused_session_count)
                 VALUES ('2026-01-15', 'code', 120000, 2, 60000, 2)",
                [],
            )
            .unwrap();

        let ranking = db.app_ranking(from, to, 10).unwrap();
        assert!(!ranking.is_empty());
        assert_eq!(ranking[0].class, "code");
        assert_eq!(ranking[0].total_ms, 120000);

        let report = db.report(from, to).unwrap();
        assert!(report.contains("code"));

        drop(db);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn delete_sessions_between_removes_children_and_sessions() {
        let db = test_db();
        let a = insert_session(
            &db,
            "kitty",
            "2025-01-10T09:00:00Z",
            Some("2025-01-10T09:10:00Z"),
        );
        let b = insert_session(
            &db,
            "firefox",
            "2025-01-11T09:00:00Z",
            Some("2025-01-11T09:10:00Z"),
        );
        // Open session (ended_at IS NULL): must be left alone.
        insert_session(&db, "kitty", "2025-01-10T10:00:00Z", None);

        for sid in [a, b] {
            db.conn
                .execute(
                    "INSERT INTO app_resources (session_id, class, sampled_at)
                     VALUES (?1, 'kitty', '2025-01-10T09:00:00Z')",
                    params![sid],
                )
                .unwrap();
            db.conn
                .execute(
                    "INSERT INTO activity_events (session_id, state, started_at)
                     VALUES (?1, 'active', '2025-01-10T09:00:00Z')",
                    params![sid],
                )
                .unwrap();
        }
        // Seed a daily_summary row that should be wiped by the rebuild.
        db.conn
            .execute(
                "INSERT INTO daily_summary (date, class, total_ms, session_count)
                 VALUES ('2025-01-10', 'kitty', 12345, 99)",
                [],
            )
            .unwrap();

        let deleted = db
            .delete_sessions_between("2025-01-10", "2025-01-11", None)
            .unwrap();
        assert_eq!(deleted, 2);

        let sessions: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(sessions, 1, "open session must remain");

        let resources: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM app_resources", [], |r| r.get(0))
            .unwrap();
        assert_eq!(resources, 0);

        let events: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM activity_events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(events, 0);

        // daily_summary was rebuilt from the remaining session only.
        let total: i64 = db
            .conn
            .query_row(
                "SELECT COALESCE(SUM(total_ms), 0) FROM daily_summary",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(total, 0, "no finished sessions => empty rebuild");
    }

    #[test]
    fn delete_sessions_between_class_filter_is_case_insensitive() {
        let db = test_db();
        insert_session(
            &db,
            "Kitty",
            "2025-01-10T09:00:00Z",
            Some("2025-01-10T09:10:00Z"),
        );
        insert_session(
            &db,
            "firefox",
            "2025-01-10T09:00:00Z",
            Some("2025-01-10T09:10:00Z"),
        );

        let deleted = db
            .delete_sessions_between("2025-01-10", "2025-01-10", Some("KITTY"))
            .unwrap();
        assert_eq!(deleted, 1);

        let remaining: String = db
            .conn
            .query_row("SELECT class FROM sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining, "firefox");
    }

    #[test]
    fn delete_sessions_before_removes_strictly_older_sessions() {
        let db = test_db();
        let old = insert_session(
            &db,
            "kitty",
            "2025-01-09T23:59:00Z",
            Some("2025-01-09T23:59:59Z"),
        );
        insert_session(
            &db,
            "firefox",
            "2025-01-10T00:00:00Z",
            Some("2025-01-10T00:10:00Z"),
        );
        // Boundary: session on the cutoff date must survive.
        insert_session(
            &db,
            "kitty",
            "2025-01-10T09:00:00Z",
            Some("2025-01-10T09:10:00Z"),
        );

        db.conn
            .execute(
                "INSERT INTO app_resources (session_id, class, sampled_at)
                 VALUES (?1, 'kitty', '2025-01-09T23:59:00Z')",
                params![old],
            )
            .unwrap();

        let deleted = db.delete_sessions_before("2025-01-10").unwrap();
        assert_eq!(deleted, 1);

        let classes: Vec<String> = db
            .conn
            .prepare("SELECT class FROM sessions ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(classes, vec!["firefox".to_string(), "kitty".to_string()]);

        let resources: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM app_resources", [], |r| r.get(0))
            .unwrap();
        assert_eq!(resources, 0);
    }

    #[test]
    fn projects_and_rules_roundtrip_and_match() {
        let db = project_test_db();
        db.ensure_projects().unwrap();

        let projects = vec![
            Project {
                id: None,
                name: "课设".to_string(),
                color: "#22d3ee".to_string(),
                sort_order: 0,
            },
            Project {
                id: None,
                name: "Open Source".to_string(),
                color: "#a855f7".to_string(),
                sort_order: 1,
            },
        ];
        let rules = vec![
            ProjectRule {
                id: None,
                project_id: 1,
                pattern: "code%".to_string(),
                priority: 0,
            },
            ProjectRule {
                id: None,
                project_id: 2,
                pattern: "firefox".to_string(),
                priority: 0,
            },
        ];
        db.set_projects(&projects, &rules).unwrap();

        let projs = db.projects().unwrap();
        assert_eq!(projs.len(), 2);

        // Classify by highest priority, case-insensitive SQLite LIKE semantics.
        assert_eq!(db.project_for_class("Code").unwrap().name, "课设");
        assert_eq!(db.project_for_class("firefox").unwrap().name, "Open Source");
        assert!(db.project_for_class("unknown").is_none());
    }

    #[test]
    fn set_projects_replaces_all() {
        let db = project_test_db();
        db.ensure_projects().unwrap();

        let p1 = Project {
            id: None,
            name: "课设".to_string(),
            color: "#22d3ee".to_string(),
            sort_order: 0,
        };
        let p2 = Project {
            id: None,
            name: "Open Source".to_string(),
            color: "#a855f7".to_string(),
            sort_order: 1,
        };
        db.set_projects(
            &[p1.clone(), p2.clone()],
            &[
                ProjectRule {
                    id: None,
                    project_id: 1,
                    pattern: "code%".to_string(),
                    priority: 0,
                },
                ProjectRule {
                    id: None,
                    project_id: 2,
                    pattern: "firefox".to_string(),
                    priority: 0,
                },
            ],
        )
        .unwrap();
        assert_eq!(db.projects().unwrap().len(), 2);
        assert_eq!(db.project_rules().unwrap().len(), 2);

        // Replace with a single project and only one rule; the other must be gone.
        // Reuse the ids as the caller would (a PUT round-trips persisted ids).
        let existing = db.projects().unwrap();
        let p1_with_id = Project {
            id: existing[0].id,
            name: existing[0].name.clone(),
            color: existing[0].color.clone(),
            sort_order: 0,
        };
        let p1_id = existing[0].id.unwrap();
        let p2_id = existing[1].id.unwrap();
        db.set_projects(
            &[p1_with_id],
            &[
                ProjectRule {
                    id: None,
                    project_id: p1_id,
                    pattern: "kitty".to_string(),
                    priority: 0,
                },
                // This rule references a removed project: it must be dropped.
                ProjectRule {
                    id: None,
                    project_id: p2_id,
                    pattern: "firefox".to_string(),
                    priority: 0,
                },
            ],
        )
        .unwrap();
        let projs = db.projects().unwrap();
        assert_eq!(projs.len(), 1);
        assert_eq!(projs[0].name, "课设");
        let rules = db.project_rules().unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].pattern, "kitty");
        assert!(db.project_for_class("kitty").is_some());
        assert!(db.project_for_class("firefox").is_none());
    }

    #[test]
    fn project_stats_buckets_and_percentages() {
        let db = project_test_db();
        db.ensure_projects().unwrap();

        let p1 = Project {
            id: None,
            name: "课设".to_string(),
            color: "#22d3ee".to_string(),
            sort_order: 0,
        };
        let p2 = Project {
            id: None,
            name: "Open Source".to_string(),
            color: "#a855f7".to_string(),
            sort_order: 1,
        };
        db.set_projects(
            &[p1.clone(), p2.clone()],
            &[
                ProjectRule {
                    id: None,
                    project_id: 1,
                    pattern: "code%".to_string(),
                    priority: 0,
                },
                ProjectRule {
                    id: None,
                    project_id: 2,
                    pattern: "firefox".to_string(),
                    priority: 0,
                },
            ],
        )
        .unwrap();

        // 3000ms → 课设, 1000ms → Open Source, 1000ms → unmatched.
        tracked_session(&db.conn, "Code", 3000);
        tracked_session(&db.conn, "firefox", 1000);
        tracked_session(&db.conn, "steam", 1000);
        // A session that is still running must be ignored.
        db.conn
            .execute(
                "INSERT INTO sessions (class, title, started_at, ended_at, duration_ms, date_local)
                 VALUES ('code', '', '2025-01-01T02:00:00Z', NULL, NULL, '2025-01-01')",
                [],
            )
            .unwrap();

        let stats = db.project_stats("2025-01-01", "2025-01-01").unwrap();
        // Sorted by total_ms desc.
        assert_eq!(stats.len(), 3);
        assert_eq!(stats[0].name, "课设");
        assert_eq!(stats[0].total_ms, 3000);
        assert_eq!(stats[0].session_count, 1);
        assert_eq!(stats[1].name, "Open Source");
        assert_eq!(stats[1].total_ms, 1000);
        assert_eq!(stats[2].name, "未分类");
        assert_eq!(stats[2].project_id, None);
        assert_eq!(stats[2].total_ms, 1000);

        // Grand total 5000ms → 60% / 20% / 20%.
        assert!((stats[0].percentage - 60.0).abs() < 0.001);
        assert!((stats[1].percentage - 20.0).abs() < 0.001);
        assert!((stats[2].percentage - 20.0).abs() < 0.001);
    }

    #[test]
    fn today_summary_idle_counts_only_gaps_after_idle_sessions() {
        let db = test_db();

        // Three consecutive sessions on the same day.
        // Gap after an active session (1 min) is NOT idle.
        // Gap after an idle-ended session (10 min) IS idle.
        db.conn
            .execute(
                "INSERT INTO sessions (class, title, started_at, ended_at, duration_ms, activity_state, focused_ms, date_local)
                 VALUES ('code', 'a', '2026-08-16T09:00:00+00:00', '2026-08-16T09:05:00+00:00', 300000, 'active', 0, '2026-08-16')",
                [],
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO sessions (class, title, started_at, ended_at, duration_ms, activity_state, focused_ms, date_local)
                 VALUES ('code', 'b', '2026-08-16T09:06:00+00:00', '2026-08-16T09:10:00+00:00', 240000, 'idle', 0, '2026-08-16')",
                [],
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO sessions (class, title, started_at, ended_at, duration_ms, activity_state, focused_ms, date_local)
                 VALUES ('code', 'c', '2026-08-16T09:20:00+00:00', '2026-08-16T09:30:00+00:00', 600000, 'active', 0, '2026-08-16')",
                [],
            )
            .unwrap();

        // daily_summary drives total_active_ms and app_ranking.
        db.conn
            .execute(
                "INSERT INTO daily_summary (date, class, total_ms, session_count, focused_ms, focused_session_count)
                 VALUES ('2026-08-16', 'code', 1140000, 3, 0, 0)",
                [],
            )
            .unwrap();

        let summary = db.today_summary("2026-08-16").unwrap();
        assert_eq!(summary.total_active_ms, 1140000);
        // Only the 10-minute gap after the idle-ended session counts.
        assert_eq!(summary.total_idle_ms, 600000);
    }

    #[test]
    fn daily_activity_zero_fills_and_orders() {
        let db = test_db();
        let today = chrono::Local::now().date_naive();
        let yesterday = today - chrono::Duration::days(1);
        let today_str = today.format("%Y-%m-%d").to_string();
        let yesterday_str = yesterday.format("%Y-%m-%d").to_string();

        // Today has data, yesterday is empty (will be zero-filled).
        db.conn
            .execute(
                "INSERT INTO daily_summary (date, class, total_ms, session_count, focused_ms, focused_session_count)
                 VALUES (?1, 'code', 3600000, 3, 1200000, 1)",
                params![today_str],
            )
            .unwrap();

        let days = db.daily_activity(7).unwrap();
        assert_eq!(days.len(), 7, "must return exactly `days` entries");
        // Oldest first, newest (today) last.
        assert_eq!(days.last().unwrap().date, today_str);
        assert_eq!(days.last().unwrap().total_ms, 3600000);
        assert_eq!(days.last().unwrap().session_count, 3);

        let yesterday_entry = days.iter().find(|d| d.date == yesterday_str).unwrap();
        assert_eq!(yesterday_entry.total_ms, 0);
        assert_eq!(yesterday_entry.session_count, 0);

        // Bounds are clamped to a minimum of 7.
        assert_eq!(db.daily_activity(1).unwrap().len(), 7);
    }

    #[test]
    fn title_stats_aggregates_and_filters() {
        let db = test_db();
        let insert = |class: &str, title: &str, ms: i64| {
            db.conn
                .execute(
                    "INSERT INTO sessions (class, title, started_at, ended_at, duration_ms, date_local)
                     VALUES (?1, ?2, '2025-01-10T09:00:00Z', '2025-01-10T10:00:00Z', ?3, '2025-01-10')",
                    params![class, title, ms],
                )
                .unwrap();
        };
        insert("code", "main.rs", 3000);
        insert("code", "README.md", 1000);
        insert("code", "", 5000); // empty title must be ignored
        insert("firefox", "github.com", 2000);

        let all = db
            .title_stats("2025-01-10", "2025-01-10", None, 50)
            .unwrap();
        assert_eq!(all.len(), 3, "empty titles are excluded");
        assert_eq!(all[0].title, "main.rs", "sorted by total_ms desc");
        assert_eq!(all[0].class, "code");
        assert_eq!(all[0].total_ms, 3000);
        assert_eq!(all[0].session_count, 1);
        assert_eq!(all[2].title, "README.md");

        let code = db
            .title_stats("2025-01-10", "2025-01-10", Some("CODE"), 50)
            .unwrap();
        assert_eq!(code.len(), 2, "class filter is case-insensitive");
        assert!(code.iter().all(|t| t.title != "github.com"));

        let none = db
            .title_stats("2025-01-10", "2025-01-10", Some("kitty"), 50)
            .unwrap();
        assert!(none.is_empty());
    }
