use chrono::Timelike;
use rusqlite::params;
use std::collections::{HashMap, HashSet};

impl super::Database {
    /// Run idempotent schema migrations.
    ///
    /// The DDL itself lives in `schema/hyprtrace.sql`, embedded by
    /// `include_str!` (see `crate::schema`) and shared verbatim with the server,
    /// so the two crates can no longer drift apart. Every statement in it is
    /// `IF NOT EXISTS`: on a database written by an older build it creates only
    /// what is still missing, and the column upgrades below bring the existing
    /// tables up to date.
    ///
    /// Step order matters and mirrors the previous hand-written chain — in
    /// particular v6 (which may rebuild the summary tables) still runs before
    /// v7 (which adds and backfills `date_local`).
    pub fn migrate(&self) -> anyhow::Result<()> {
        self.conn.execute_batch(crate::schema::SCHEMA_SQL)?;

        self.migrate_v2()?;
        self.migrate_v3()?;
        self.migrate_v6()?;
        self.migrate_v7()?;
        Ok(())
    }

    /// Repair sessions/events that were recorded with a negative duration when
    /// the wall clock jumped backwards (e.g. dual-boot with Windows leaving the
    /// RTC in local time), and rebuild the summary tables from the repaired
    /// sessions so no negative totals remain.
    pub(super) fn migrate_v6(&self) -> anyhow::Result<()> {
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

    /// Add a local-calendar-date column to the tables keyed by date, and backfill
    /// any existing rows. Sessions/disruptions/activity_events store their
    /// timestamps as UTC RFC3339, but the UI (and daily/hourly summaries) are
    /// keyed by the user's LOCAL calendar date. An explicit `date_local` column
    /// lets every `date(...)` filter use the same value the frontend requests,
    /// without baking a timezone into the stored instant.
    fn migrate_v7(&self) -> anyhow::Result<()> {
        self.add_column_if_missing("sessions", "date_local", "TEXT", "''")?;
        self.add_column_if_missing("disruptions", "date_local", "TEXT", "''")?;
        self.add_column_if_missing("activity_events", "date_local", "TEXT", "''")?;
        self.add_column_if_missing("app_resources", "date_local", "TEXT", "''")?;

        self.backfill_date_local("sessions", "started_at")?;
        self.backfill_date_local("disruptions", "occurred_at")?;
        self.backfill_date_local("activity_events", "started_at")?;
        self.backfill_date_local("app_resources", "sampled_at")?;

        Ok(())
    }

    /// Compute the local calendar date (`YYYY-MM-DD`) of a stored row from its
    /// UTC timestamp column, for any rows whose `date_local` is still empty.
    ///
    /// All updates run inside a SINGLE `BEGIN IMMEDIATE` transaction. One
    /// autocommit UPDATE per row costs a fsync per commit (synchronous=FULL),
    /// which measured ~200ms/row on some disks — a backfill of a few thousand
    /// rows therefore held the SQLite write lock for tens of minutes. Long
    /// enough that the server (which also writes at startup) hit its 5s
    /// `busy_timeout`, exited with "database is locked", and systemd crash-
    /// looped both services for the whole migration window.
    fn backfill_date_local(&self, table: &str, ts_col: &str) -> anyhow::Result<()> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT id, {ts_col} FROM {table} WHERE date_local = ''"
        ))?;
        let rows: Vec<(i64, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<_, _>>()?;
        drop(stmt);
        if rows.is_empty() {
            return Ok(());
        }

        let tx = rusqlite::Transaction::new_unchecked(
            &self.conn,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        let mut upd = tx.prepare(&format!("UPDATE {table} SET date_local = ?1 WHERE id = ?2"))?;
        let mut updated = 0usize;
        for (id, ts) in rows {
            if let Some(date) = local_date_from_rfc3339(&ts) {
                upd.execute(params![date, id])?;
                updated += 1;
            }
        }
        drop(upd);
        tx.commit()?;

        if updated > 0 {
            log::info!("Backfilled date_local for {table}: {updated} row(s)");
        }
        Ok(())
    }

    /// Recompute daily_summary and hourly_summary from the `sessions` table.
    /// Used by migrate_v6 to clean up summaries polluted by negative durations.
    ///
    /// The DELETE + INSERTs run inside a single transaction for the same reason
    /// as `backfill_date_local`: per-row autocommit statements cost a fsync
    /// each and would otherwise hold the write lock for minutes.
    fn rebuild_summaries_from_sessions(&self) -> anyhow::Result<()> {
        let tx = rusqlite::Transaction::new_unchecked(
            &self.conn,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        self.write_summaries(&tx, None)?;
        tx.commit()?;
        Ok(())
    }

    /// The body of the rebuild, executed on a connection the caller already
    /// owns a transaction for. Split out so `finalize_orphaned_sessions` can
    /// close its sessions and fix the summaries in ONE transaction: closing them
    /// and then being interrupted before the summaries follow would leave the
    /// two disagreeing with no trace of it.
    pub(super) fn write_summaries(
        &self,
        conn: &rusqlite::Connection,
        only_dates: Option<&HashSet<String>>,
    ) -> anyhow::Result<()> {
        // An empty scope has nothing to delete and nothing to recompute; the
        // `IN ()` it would produce is not valid SQL either.
        if matches!(only_dates, Some(dates) if dates.is_empty()) {
            return Ok(());
        }

        let mut stmt = conn.prepare(
            "SELECT started_at, class, duration_ms, COALESCE(focused_ms, 0)
             FROM sessions
             WHERE ended_at IS NOT NULL AND duration_ms >= 0",
        )?;
        let rows: Vec<(String, String, i64, i64)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })?
            .collect::<Result<_, _>>()?;
        drop(stmt);

        // Bucket by the LOCAL calendar date of the session start.
        let mut daily: HashMap<(String, String), (i64, i64, i64, i64)> = HashMap::new();
        let mut hourly: HashMap<(String, u8, String), (i64, i64, i64)> = HashMap::new();
        for (started_at, class, duration_ms, focused_ms) in rows {
            if let Some(dt) = chrono::DateTime::parse_from_rfc3339(&started_at).ok() {
                let local = dt.with_timezone(&chrono::Local);
                let date = local.format("%Y-%m-%d").to_string();
                if let Some(dates) = only_dates {
                    if !dates.contains(&date) {
                        continue;
                    }
                }
                let hour = local.hour() as u8;
                let d = daily
                    .entry((date.clone(), class.clone()))
                    .or_insert((0, 0, 0, 0));
                d.0 += duration_ms;
                d.1 += 1;
                d.2 += focused_ms;
                if focused_ms > 0 {
                    d.3 += 1;
                }
                let h = hourly.entry((date, hour, class)).or_insert((0, 0, 0));
                h.0 += duration_ms;
                h.1 += 1;
                h.2 += focused_ms;
            }
        }

        // A scoped rebuild must only clear the days it is about to recompute —
        // wiping the other days would erase them without putting anything back.
        let mut scope: Vec<String> = match only_dates {
            None => Vec::new(),
            Some(dates) => dates.iter().cloned().collect(),
        };
        scope.sort();
        let (daily_delete, hourly_delete) = if scope.is_empty() {
            (
                "DELETE FROM daily_summary".to_string(),
                "DELETE FROM hourly_summary".to_string(),
            )
        } else {
            let placeholders = vec!["?"; scope.len()].join(",");
            (
                format!("DELETE FROM daily_summary WHERE date IN ({placeholders})"),
                format!("DELETE FROM hourly_summary WHERE date IN ({placeholders})"),
            )
        };
        let scope_params: Vec<&dyn rusqlite::types::ToSql> = scope
            .iter()
            .map(|d| d as &dyn rusqlite::types::ToSql)
            .collect();
        conn.execute(&daily_delete, scope_params.as_slice())?;
        for ((date, class), (total_ms, session_count, focused_ms, focused_count)) in daily {
            conn.execute(
                "INSERT INTO daily_summary (date, class, total_ms, session_count, focused_ms, focused_session_count)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![date, class, total_ms, session_count, focused_ms, focused_count],
            )?;
        }

        conn.execute(&hourly_delete, scope_params.as_slice())?;
        for ((date, hour, class), (total_ms, session_count, focused_ms)) in hourly {
            conn.execute(
                "INSERT INTO hourly_summary (date, hour, class, total_ms, session_count, focused_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![date, hour as i64, class, total_ms, session_count, focused_ms],
            )?;
        }
        Ok(())
    }

    /// v3 column upgrade. The table v3 introduced (`app_resources`) is created
    /// by the shared schema file, which runs first — this only reaches database
    /// files that predate the shared file.
    fn migrate_v3(&self) -> anyhow::Result<()> {
        self.add_column_if_missing("sessions", "pid", "INTEGER", "NULL")?;
        Ok(())
    }

    /// v2 column upgrades. The tables v2 introduced (`activity_events`,
    /// `hourly_summary`) are created by the shared schema file, which runs
    /// first — these ALTERs only reach database files that predate it.
    fn migrate_v2(&self) -> anyhow::Result<()> {
        self.add_column_if_missing("sessions", "activity_state", "TEXT", "'active'")?;
        self.add_column_if_missing("sessions", "focused_ms", "INTEGER", "0")?;

        self.add_column_if_missing("daily_summary", "focused_ms", "INTEGER", "0")?;
        self.add_column_if_missing("daily_summary", "focused_session_count", "INTEGER", "0")?;

        self.add_column_if_missing("ai_conversations", "complete", "INTEGER", "1")?;

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
}

/// Local calendar date (`YYYY-MM-DD`) of a stored UTC RFC3339 timestamp.
fn local_date_from_rfc3339(ts: &str) -> Option<String> {
    chrono::DateTime::parse_from_rfc3339(ts).ok().map(|dt| {
        dt.with_timezone(&chrono::Local)
            .format("%Y-%m-%d")
            .to_string()
    })
}
