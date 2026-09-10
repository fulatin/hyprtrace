use super::migrate::local_date_from_rfc3339;
use chrono::Timelike;
use rusqlite::params;

/// What one retention pass removed, per table.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RetentionOutcome {
    pub sessions: usize,
    pub disruptions: usize,
    pub app_resources: usize,
    pub activity_events: usize,
}

impl super::Database {
    /// Delete child rows first (FK integrity), then the matching finished
    /// sessions, then rebuild both summary tables from the remaining sessions.
    fn delete_matching_sessions(
        &self,
        where_clause: &str,
        params: &[Box<dyn rusqlite::types::ToSql>],
    ) -> anyhow::Result<usize> {
        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|b| b.as_ref()).collect();

        let delete_children = |table: &str| -> anyhow::Result<()> {
            self.conn.execute(
                &format!(
                    "DELETE FROM {table} WHERE session_id IN (
                        SELECT id FROM sessions WHERE {where_clause}
                    )"
                ),
                params_refs.as_slice(),
            )?;
            Ok(())
        };
        // Children reference sessions(id) via FOREIGN KEY, so delete them first.
        delete_children("app_resources")?;
        delete_children("activity_events")?;

        let deleted = self.conn.execute(
            &format!("DELETE FROM sessions WHERE {where_clause}"),
            params_refs.as_slice(),
        )?;

        self.rebuild_daily_summary()?;
        self.rebuild_hourly_summary()?;

        Ok(deleted)
    }

    /// Delete finished sessions whose `started_at` falls within the inclusive
    /// `[from, to]` date range, optionally restricted to a case-insensitive
    /// `class` match. Returns the number of deleted sessions.
    pub fn delete_sessions_between(
        &self,
        from: &str,
        to: &str,
        class: Option<&str>,
    ) -> anyhow::Result<usize> {
        let (where_clause, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match class {
            Some(c) => (
                "date_local BETWEEN ?1 AND ?2
                 AND lower(class) = lower(?3)
                 AND ended_at IS NOT NULL"
                    .to_string(),
                vec![
                    Box::new(from.to_string()),
                    Box::new(to.to_string()),
                    Box::new(c.to_string()),
                ],
            ),
            None => (
                "date_local BETWEEN ?1 AND ?2 AND ended_at IS NOT NULL".to_string(),
                vec![Box::new(from.to_string()), Box::new(to.to_string())],
            ),
        };
        self.delete_matching_sessions(&where_clause, &params)
    }

    /// Delete finished sessions whose `started_at` is strictly before
    /// `cutoff_date` (no class filter). Returns the number of deleted sessions.
    pub fn delete_sessions_before(&self, cutoff_date: &str) -> anyhow::Result<usize> {
        let where_clause = "date_local < ?1 AND ended_at IS NOT NULL".to_string();
        let params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(cutoff_date.to_string())];
        self.delete_matching_sessions(&where_clause, &params)
    }

    /// Delete the auxiliary rows older than `cutoff_date` that deleting a
    /// session does NOT reach.
    ///
    /// Deleting a session takes its `app_resources` and `activity_events` with
    /// it, so the retention policy looked like it covered everything. It did
    /// not:
    ///
    ///   * `disruptions` has no session to cascade from and was never deleted by
    ///     any code path at all — it is the largest auxiliary table in practice.
    ///   * `app_resources` and `activity_events` outlive the policy whenever
    ///     their session survives it: a session the daemon left open, a session
    ///     whose row is kept, or a row whose `session_id` is NULL.
    ///
    /// Rows are matched on the local `date_local`, the column the rest of the
    /// retention code filters on. A row whose `date_local` is still empty is
    /// deliberately kept: it has no date the policy can be applied to, and
    /// guessing one would delete data the cutoff does not clearly cover.
    pub fn prune_auxiliary_before(&self, cutoff_date: &str) -> anyhow::Result<RetentionOutcome> {
        let mut outcome = RetentionOutcome::default();
        for table in ["disruptions", "app_resources", "activity_events"] {
            let deleted = self.conn.execute(
                &format!("DELETE FROM {table} WHERE date_local <> '' AND date_local < ?1"),
                params![cutoff_date],
            )?;
            match table {
                "disruptions" => outcome.disruptions = deleted,
                "app_resources" => outcome.app_resources = deleted,
                _ => outcome.activity_events = deleted,
            }
        }
        Ok(outcome)
    }

    /// One retention pass: the finished sessions older than `cutoff_date` (with
    /// their children, and the summaries rebuilt from what is left) plus the
    /// auxiliary rows no session deletion reaches.
    pub fn apply_retention(&self, cutoff_date: &str) -> anyhow::Result<RetentionOutcome> {
        let sessions = self.delete_sessions_before(cutoff_date)?;
        let mut outcome = self.prune_auxiliary_before(cutoff_date)?;
        outcome.sessions = sessions;
        Ok(outcome)
    }

    /// Rewrite `daily_summary` from `sessions`.
    ///
    /// The DELETE and the INSERT must land together. Without a transaction an
    /// interruption between them leaves the table EMPTY, and `hourly_breakdown`
    /// (and the dashboard) trust `daily_summary`/`hourly_summary` as soon as a
    /// date has one row — so the charts would show nothing at all rather than
    /// something stale, with no error anywhere.
    /// Rewrite `daily_summary` from `sessions`.
    ///
    /// The rules are the ones `end_session` applies incrementally, so that a
    /// rebuild reproduces the numbers the incremental path already wrote. The
    /// two used to disagree on all three counts:
    ///
    ///   * every finished session counts, a zero-duration one included —
    ///     `end_session` increments `session_count` for it, so filtering on
    ///     `duration_ms > 0` silently changed the day's session count.
    ///   * the bucket is the LOCAL date of `started_at`, derived here instead of
    ///     read from `date_local`, so a row whose column is still empty is
    ///     bucketed correctly rather than dropped — filtering on
    ///     `date_local <> ''` could rebuild an EMPTY table, which is what the
    ///     server's `migrate_v6` did when it ran before `migrate_v7` had
    ///     backfilled that column.
    ///   * `focused_ms` is re-derived from the session duration with the
    ///     threshold in force NOW, so a rebuild is "recompute with today's
    ///     rules": lowering `focused_threshold_seconds` and rebuilding does
    ///     re-score history, which is what the option is for.
    ///     `focused_session_count` counts the sessions that contributed focus —
    ///     `focused_ms > 0` — the same rule the incremental path uses.
    ///
    /// The daemon keeps its own copy of these rules (`db/migrations.rs`); the
    /// two must stay in step, which is what
    /// `rebuild_matches_the_incremental_path` in each crate's tests pins down.
    pub fn rebuild_daily_summary(&self) -> anyhow::Result<()> {
        let threshold = self.focused_threshold_ms;
        let mut stmt = self.conn.prepare(
            "SELECT started_at, class, duration_ms
             FROM sessions
             WHERE ended_at IS NOT NULL AND duration_ms >= 0",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;

        let mut agg: std::collections::HashMap<(String, String), (i64, i64, i64, i64)> =
            std::collections::HashMap::new();
        for r in rows {
            let (started_at, class, duration_ms) = r?;
            let Some(date) = local_date_from_rfc3339(&started_at) else {
                continue;
            };
            let focused_ms = std::cmp::max(0, duration_ms - threshold);
            let entry = agg.entry((date, class)).or_insert((0, 0, 0, 0));
            entry.0 += duration_ms;
            entry.1 += 1;
            entry.2 += focused_ms;
            if focused_ms > 0 {
                entry.3 += 1;
            }
        }
        drop(stmt);

        let tx = rusqlite::Transaction::new_unchecked(
            &self.conn,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        tx.execute("DELETE FROM daily_summary", [])?;
        for ((date, class), (total_ms, session_count, focused_ms, focused_count)) in agg {
            tx.execute(
                "INSERT INTO daily_summary (date, class, total_ms, session_count, focused_ms, focused_session_count)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![date, class, total_ms, session_count, focused_ms, focused_count],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Rewrite `hourly_summary` from `sessions`. Transactional for the same
    /// reason as `rebuild_daily_summary` — and here it matters more, because the
    /// rows are inserted one at a time from the Rust-side aggregation.
    ///
    /// Same rules as `rebuild_daily_summary`; the bucket is the LOCAL start hour.
    pub fn rebuild_hourly_summary(&self) -> anyhow::Result<()> {
        // Aggregate in Rust so we can bucket by LOCAL hour (chrono::Local),
        // consistent with the sessions-based fallback and the daemon's incremental upsert.
        let threshold = self.focused_threshold_ms;
        let mut stmt = self.conn.prepare(
            "SELECT started_at, class, duration_ms
             FROM sessions
             WHERE ended_at IS NOT NULL AND duration_ms >= 0",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;

        let mut agg: std::collections::HashMap<(String, u8, String), (i64, i64, i64)> =
            std::collections::HashMap::new();
        for r in rows {
            let (started_at, class, duration_ms) = r?;
            let Some(local) = chrono::DateTime::parse_from_rfc3339(&started_at)
                .ok()
                .map(|dt| dt.with_timezone(&chrono::Local))
            else {
                continue;
            };
            let date = local.format("%Y-%m-%d").to_string();
            let hour = local.hour() as u8;
            let entry = agg.entry((date, hour, class)).or_insert((0, 0, 0));
            entry.0 += duration_ms;
            entry.1 += 1;
            entry.2 += std::cmp::max(0, duration_ms - threshold);
        }
        drop(stmt);

        let tx = rusqlite::Transaction::new_unchecked(
            &self.conn,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        tx.execute("DELETE FROM hourly_summary", [])?;
        for ((date, hour, class), (total_ms, session_count, focused_ms)) in agg {
            tx.execute(
                "INSERT INTO hourly_summary (date, hour, class, total_ms, session_count, focused_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![date, hour as i64, class, total_ms, session_count, focused_ms],
            )?;
        }
        tx.commit()?;

        Ok(())
    }
}
