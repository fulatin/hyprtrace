use rusqlite::params;

impl super::Database {
    /// Run idempotent schema migrations.
    ///
    /// The DDL is shared with the daemon through `schema/hyprtrace.sql` +
    /// `include_str!` (see `crate::schema`), so there is no longer a second
    /// hand-maintained copy to keep in sync.
    pub fn migrate(&self) -> anyhow::Result<()> {
        // Every statement in the shared file is `IF NOT EXISTS`: on a database
        // written by an older build it creates only what is still missing, and
        // the column upgrades below bring the existing tables up to date.
        //
        // This matters because the server can start before the daemon has ever
        // run (install.sh enables both services with no ordering guarantee).
        // Without these migrations every API would fail with "no such table"
        // until the daemon writes the database once.
        //
        // Step order: v7 runs BEFORE v6. v6 may rebuild the summary tables, and
        // that rebuild now buckets by the local date derived from `started_at`,
        // but the sequence it replaced read the `date_local` column — which v7
        // is what adds and backfills. Running v6 first therefore rebuilt the
        // summaries by filtering on a column that was empty for every existing
        // row, i.e. it wiped `daily_summary` and left it empty. v6's own repair
        // (clamping negative durations) does not depend on v7, so swapping the
        // two is safe.
        self.conn.execute_batch(crate::schema::SCHEMA_SQL)?;

        self.migrate_v2()?;
        self.migrate_v3()?;
        self.migrate_v7()?;
        self.migrate_v6()?;
        Ok(())
    }

    fn migrate_v2(&self) -> anyhow::Result<()> {
        self.add_column_if_missing("sessions", "activity_state", "TEXT", "'active'")?;
        self.add_column_if_missing("sessions", "focused_ms", "INTEGER", "0")?;

        self.add_column_if_missing("daily_summary", "focused_ms", "INTEGER", "0")?;
        self.add_column_if_missing("daily_summary", "focused_session_count", "INTEGER", "0")?;

        self.add_column_if_missing("ai_conversations", "complete", "INTEGER", "1")?;

        // `activity_events` and `hourly_summary` come from the shared schema.
        Ok(())
    }

    fn migrate_v3(&self) -> anyhow::Result<()> {
        self.add_column_if_missing("sessions", "pid", "INTEGER", "NULL")?;
        // `app_resources` comes from the shared schema.
        Ok(())
    }

    /// The tables v4/v5 introduced (`disruptions`, `goals`) are created by the
    /// shared schema file, so there is no per-step copy of their DDL here.

    /// Mirrors the daemon's v6: clamp negative durations left by a wall-clock
    /// jump and rebuild the summary tables from the repaired sessions. The
    /// server can start before the daemon has ever run (no ordering guarantee
    /// in install.sh), so it must run the same repair as the daemon, otherwise
    /// negative summaries linger when only the server initialised the DB.
    fn migrate_v6(&self) -> anyhow::Result<()> {
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

        let negative_summary_rows: i64 = self.conn.query_row(
            "SELECT (SELECT COUNT(*) FROM daily_summary WHERE total_ms < 0 OR focused_ms < 0)
                  + (SELECT COUNT(*) FROM hourly_summary WHERE total_ms < 0 OR focused_ms < 0)",
            [],
            |row| row.get(0),
        )?;

        if sessions_repaired > 0 || events_repaired > 0 || negative_summary_rows > 0 {
            self.rebuild_daily_summary()?;
            self.rebuild_hourly_summary()?;
        }
        Ok(())
    }

    /// Mirrors the daemon's v7: add the local-calendar-date column to the
    /// date-keyed tables and backfill existing rows, inside one transaction.
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

    /// Compute the local calendar date (`YYYY-MM-DD`) of a stored row from its
    /// UTC timestamp column, for any rows whose `date_local` is still empty.
    /// Runs in one transaction (a per-row autocommit loop would be far slower).
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
}

/// Local calendar date (`YYYY-MM-DD`) of a stored UTC RFC3339 timestamp.
///
/// Shared with `db::retention`, whose rebuilds derive the bucket from
/// `started_at` instead of reading `date_local`.
pub(super) fn local_date_from_rfc3339(ts: &str) -> Option<String> {
    chrono::DateTime::parse_from_rfc3339(ts).ok().map(|dt| {
        dt.with_timezone(&chrono::Local)
            .format("%Y-%m-%d")
            .to_string()
    })
}
