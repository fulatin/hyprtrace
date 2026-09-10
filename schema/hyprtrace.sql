-- HyprTrace canonical database schema — SINGLE SOURCE OF TRUTH.
--
-- Both `crates/hyprtrace-daemon` and `crates/hyprtrace-server` embed this file
-- verbatim with `include_str!` (see each crate's `src/schema.rs`) and run it at
-- startup from `Database::migrate()`. Before this file existed the two crates
-- kept hand-maintained copies that had to be edited in lockstep ("KEEP IN
-- SYNC"), and the copies had already diverged once in the v6 rebuild path.
--
-- Rules for editing:
--   * Every statement must stay idempotent (`IF NOT EXISTS`) — `migrate()` runs
--     the whole file on every start and there is no version gate.
--   * This file describes the shape a FRESH database gets. It must stay
--     identical to what the legacy `add_column_if_missing` calls in each
--     crate produce for an OLD database, otherwise a new install and an
--     upgraded install would carry different schemas. Columns added by those
--     legacy calls are therefore declared here as nullable-with-default, which
--     is exactly what `ALTER TABLE ... ADD COLUMN` yields.
--   * `schema/expected-columns.txt` is the committed snapshot of the resulting
--     shape and is asserted by a test in each crate. If you change this file,
--     regenerate that snapshot (see the test's failure message).
--   * Server-owned tables (`app_categories`, `projects`, `project_rules`) are
--     deliberately NOT here: the server creates and seeds them itself in
--     `ensure_categories()` / `ensure_projects()`.
--
-- Dates: `sessions.started_at` / `disruptions.occurred_at` hold UTC RFC3339
-- strings; `date_local` holds the LOCAL calendar date (`YYYY-MM-DD`) derived
-- from them. `daily_summary.date` / `hourly_summary.date` are local dates too.
--
-- FOCUS IS NOT READ FROM THIS SCHEMA ANY MORE. The four `focused_ms` columns and
-- `focused_session_count` record a per-session threshold
-- (`max(0, duration_ms - focused_threshold_seconds)`), which is 0 for every
-- session shorter than the threshold — with a median session of about a second,
-- that was every session, so the figure was always 0. Focus time is now derived
-- on demand from focus blocks (`insights::focus` in the server: a run of
-- consecutive same-class sessions on one local date adding up to 15 minutes).
-- The columns are still written by the daemon and rebuilt by the server so
-- nothing that reads them breaks, but no authoritative figure comes from them.

CREATE TABLE IF NOT EXISTS sessions (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    class          TEXT NOT NULL,
    title          TEXT NOT NULL DEFAULT '',
    workspace      TEXT,
    started_at     TEXT NOT NULL,
    ended_at       TEXT,
    duration_ms    INTEGER DEFAULT 0,
    -- added by migrate_v2 / v3 / v7
    activity_state TEXT DEFAULT 'active',
    -- legacy threshold figure; focus is derived, see the header note
    focused_ms     INTEGER DEFAULT 0,
    pid            INTEGER DEFAULT NULL,
    date_local     TEXT DEFAULT ''
);

CREATE INDEX IF NOT EXISTS idx_sessions_class ON sessions(class);
CREATE INDEX IF NOT EXISTS idx_sessions_started ON sessions(started_at);

CREATE TABLE IF NOT EXISTS daily_summary (
    id                    INTEGER PRIMARY KEY AUTOINCREMENT,
    date                  TEXT NOT NULL,
    class                 TEXT NOT NULL,
    total_ms              INTEGER NOT NULL DEFAULT 0,
    session_count         INTEGER NOT NULL DEFAULT 0,
    -- added by migrate_v2; legacy threshold figure, see the header note
    focused_ms            INTEGER DEFAULT 0,
    focused_session_count INTEGER DEFAULT 0,
    UNIQUE(date, class)
);

CREATE INDEX IF NOT EXISTS idx_daily_summary_date ON daily_summary(date);

CREATE TABLE IF NOT EXISTS hourly_summary (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    date          TEXT NOT NULL,
    hour          INTEGER NOT NULL,
    class         TEXT NOT NULL,
    total_ms      INTEGER NOT NULL DEFAULT 0,
    session_count INTEGER NOT NULL DEFAULT 0,
    -- legacy threshold figure, see the header note
    focused_ms    INTEGER NOT NULL DEFAULT 0,
    UNIQUE(date, hour, class)
);

CREATE INDEX IF NOT EXISTS idx_hourly_summary_date ON hourly_summary(date);

CREATE TABLE IF NOT EXISTS ai_conversations (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at TEXT NOT NULL,
    role       TEXT NOT NULL,
    content    TEXT NOT NULL,
    model      TEXT NOT NULL,
    -- added by migrate_v2
    complete   INTEGER DEFAULT 1
);

CREATE TABLE IF NOT EXISTS activity_events (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id  INTEGER,
    state       TEXT NOT NULL,
    started_at  TEXT NOT NULL,
    ended_at    TEXT,
    duration_ms INTEGER DEFAULT 0,
    -- added by migrate_v7
    date_local  TEXT DEFAULT '',
    FOREIGN KEY (session_id) REFERENCES sessions(id)
);

CREATE INDEX IF NOT EXISTS idx_activity_events_session ON activity_events(session_id);

CREATE TABLE IF NOT EXISTS app_resources (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id INTEGER,
    class      TEXT NOT NULL,
    sampled_at TEXT NOT NULL,
    cpu_pct    REAL,
    mem_kb     INTEGER,
    -- added by migrate_v7
    date_local TEXT DEFAULT '',
    FOREIGN KEY (session_id) REFERENCES sessions(id)
);

CREATE INDEX IF NOT EXISTS idx_app_resources_class ON app_resources(class);
CREATE INDEX IF NOT EXISTS idx_app_resources_session ON app_resources(session_id);

CREATE TABLE IF NOT EXISTS disruptions (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    kind        TEXT NOT NULL,
    app         TEXT,
    summary     TEXT,
    occurred_at TEXT NOT NULL,
    -- added by migrate_v7
    date_local  TEXT DEFAULT ''
);

CREATE INDEX IF NOT EXISTS idx_disruptions_at ON disruptions(occurred_at);
CREATE INDEX IF NOT EXISTS idx_disruptions_kind ON disruptions(kind);

CREATE TABLE IF NOT EXISTS goals (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    name            TEXT NOT NULL,
    target_type     TEXT NOT NULL DEFAULT 'all',
    target_key      TEXT,
    daily_target_ms INTEGER NOT NULL,
    enabled         INTEGER NOT NULL DEFAULT 1
);
