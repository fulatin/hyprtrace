use chrono::Timelike;
use rusqlite::params;
use std::collections::HashSet;
use std::time::Instant;

impl super::Database {
    pub fn start_session(
        &self,
        class: &str,
        title: &str,
        workspace: &str,
        pid: Option<i32>,
    ) -> anyhow::Result<i64> {
        let now = chrono::Utc::now();
        let date_local = now
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d")
            .to_string();
        self.conn.execute(
            "INSERT INTO sessions (class, title, workspace, started_at, activity_state, pid, date_local)
             VALUES (?1, ?2, ?3, ?4, 'active', ?5, ?6)",
            params![class, title, workspace, now.to_rfc3339(), pid, date_local],
        )?;
        let id = self.conn.last_insert_rowid();
        if let Ok(mut starts) = self.monotonic_starts.lock() {
            starts.insert(id, Instant::now());
        }
        self.save_activity_event(id, "active")?;
        Ok(id)
    }

    /// Elapsed milliseconds since `session_id` was started.
    ///
    /// Uses the monotonic `Instant` recorded by `start_session` so clock jumps
    /// (e.g. dual-boot Windows/Linux RTC skew) cannot produce negative values.
    /// Falls back to wall-clock parsing for sessions started by a previous
    /// daemon process (rare), still clamped to >= 0.
    pub fn session_elapsed_ms(&self, session_id: i64) -> anyhow::Result<i64> {
        if let Ok(starts) = self.monotonic_starts.lock() {
            if let Some(started) = starts.get(&session_id) {
                let ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
                return Ok(ms.max(0));
            }
        }

        let started_at: String = self.conn.query_row(
            "SELECT started_at FROM sessions WHERE id = ?1",
            params![session_id],
            |row| row.get(0),
        )?;
        let started =
            chrono::DateTime::parse_from_rfc3339(&started_at)?.with_timezone(&chrono::Utc);
        Ok((chrono::Utc::now() - started).num_milliseconds().max(0))
    }

    pub fn end_session(&self, session_id: i64) -> anyhow::Result<()> {
        let now = chrono::Utc::now();
        let now_str = now.to_rfc3339();

        let started_at: String = self.conn.query_row(
            "SELECT started_at FROM sessions WHERE id = ?1",
            params![session_id],
            |row| row.get(0),
        )?;

        let started: chrono::DateTime<chrono::Utc> =
            chrono::DateTime::parse_from_rfc3339(&started_at)?.with_timezone(&chrono::Utc);
        // Prefer the process-monotonic elapsed time: when the wall clock jumps
        // backwards the RFC3339 timestamps would yield a negative duration.
        let duration_ms = self.session_elapsed_ms(session_id).unwrap_or_else(|_| {
            // Fallback (should never be hit for an in-memory session): clamp a
            // wall-clock-negative duration to zero instead of storing negatives.
            (now - started).num_milliseconds().max(0)
        });

        // If the wall clock jumped while the session was running (NTP step,
        // dual-boot RTC skew), the recorded started_at no longer lines up with
        // the monotonic duration. Rewrite it from `now - duration` so the
        // stored timestamps stay ordered and the timeline remains usable.
        let wall_delta_ms = (now - started).num_milliseconds();
        let clock_jumped = (wall_delta_ms - duration_ms).abs() > 1000;
        let started = if clock_jumped {
            now - chrono::Duration::milliseconds(duration_ms)
        } else {
            started
        };

        // Bucket the session on the LOCAL day it STARTED. This matches the
        // server's date_local field and the local-date queries the frontend
        // sends, so a session that spans midnight is attributed to the day the
        // user actually started it (in their own timezone).

        let focused_ms = std::cmp::max(0, duration_ms - self.focused_threshold_ms);

        let session_date = started
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d")
            .to_string();

        // Ending a session writes several tables (session row, daily_summary,
        // hourly_summary, activity_events). Wrap all of it in ONE transaction
        // so an interruption or mid-way failure can no longer leave a
        // half-written state (e.g. session closed but summaries never updated).
        // unchecked_transaction is fine here: the Database lives behind a
        // Mutex, so no other thread touches this connection concurrently.
        let tx = self.conn.unchecked_transaction()?;

        if clock_jumped {
            tx.execute(
                "UPDATE sessions SET ended_at = ?1, duration_ms = ?2, focused_ms = ?3, started_at = ?4, date_local = ?5 WHERE id = ?6",
                params![now_str, duration_ms, focused_ms, started.to_rfc3339(), session_date, session_id],
            )?;
        } else {
            tx.execute(
                "UPDATE sessions SET ended_at = ?1, duration_ms = ?2, focused_ms = ?3, date_local = ?4 WHERE id = ?5",
                params![now_str, duration_ms, focused_ms, session_date, session_id],
            )?;
        }

        let class: String = tx.query_row(
            "SELECT class FROM sessions WHERE id = ?1",
            params![session_id],
            |row| row.get(0),
        )?;

        let activity_state: String = tx.query_row(
            "SELECT activity_state FROM sessions WHERE id = ?1",
            params![session_id],
            |row| row.get(0),
        )?;
        let is_focused_session = if activity_state == "focused" {
            1i64
        } else {
            0i64
        };

        tx.execute(
            "INSERT INTO daily_summary (date, class, total_ms, session_count, focused_ms, focused_session_count)
             VALUES (?1, ?2, ?3, 1, ?4, ?5)
             ON CONFLICT(date, class) DO UPDATE SET
               total_ms = total_ms + ?3,
               session_count = session_count + 1,
               focused_ms = focused_ms + ?4,
               focused_session_count = focused_session_count + ?5",
             params![session_date, class, duration_ms, focused_ms, is_focused_session],
        )?;

        // Maintain hourly_summary incrementally so the server's fast path stays
        // fresh. Bucket by the session's LOCAL start hour (matching date_local).
        // Lenient by design (pre-existing behavior): a failed hourly upsert
        // only logs; the rest of the end-session still commits.
        let local_hour = started.with_timezone(&chrono::Local).hour() as i64;
        if let Err(e) = tx.execute(
            "INSERT INTO hourly_summary (date, hour, class, total_ms, session_count, focused_ms)
             VALUES (?1, ?2, ?3, ?4, 1, ?5)
             ON CONFLICT(date, hour, class) DO UPDATE SET
               total_ms = total_ms + ?4,
               session_count = session_count + 1,
               focused_ms = focused_ms + ?5",
            params![session_date, local_hour, class, duration_ms, focused_ms],
        ) {
            log::warn!("Failed to upsert hourly_summary: {}", e);
        }

        // Runs on the same connection, so these writes join the transaction.
        self.close_activity_event(session_id);

        tx.commit()?;
        // Only drop the monotonic start once the end has durably committed;
        // on a rollback the session is still open and must stay resumable.
        if let Ok(mut starts) = self.monotonic_starts.lock() {
            starts.remove(&session_id);
        }
        Ok(())
    }

    pub fn update_session_state(&self, session_id: i64, state: &str) -> anyhow::Result<()> {
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
        let now = chrono::Utc::now();
        let date_local = now
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d")
            .to_string();
        self.conn.execute(
            "INSERT INTO activity_events (session_id, state, started_at, date_local) VALUES (?1, ?2, ?3, ?4)",
            params![session_id, state, now.to_rfc3339(), date_local],
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
                let dur = (now - sa_utc).num_milliseconds().max(0);
                let _ = self.conn.execute(
                    "UPDATE activity_events SET ended_at = ?1, duration_ms = ?2
                     WHERE session_id = ?3 AND ended_at IS NULL",
                    params![now_str, dur, session_id],
                );
            }
        }
    }

    pub fn update_ongoing_focused_ms(&self, session_id: i64) -> anyhow::Result<()> {
        let duration_ms = self.session_elapsed_ms(session_id)?;
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

    /// Close sessions left open by an unclean shutdown (crash, power loss).
    ///
    /// These used to be deleted outright, which threw away the usage they had
    /// already recorded. They are finalised instead: the end time is taken from
    /// the last activity event belonging to the session, falling back to the
    /// session start when it never recorded one. The monotonic elapsed time is
    /// not available here (those markers live in memory only), so the duration
    /// is wall-clock based and can differ slightly from a session closed in
    /// flight — losing that precision is vastly better than losing the record.
    ///
    /// The summaries are recomputed for the affected days afterwards. Without
    /// that step the finalised sessions were listed but counted in no total:
    /// `daily_summary`/`hourly_summary` are maintained incrementally by
    /// `end_session`, which this path does not go through.
    pub fn finalize_orphaned_sessions(&self) -> anyhow::Result<usize> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, started_at FROM sessions WHERE ended_at IS NULL")?;
        let orphans: Vec<(i64, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);

        let now = chrono::Utc::now();
        let mut count = 0usize;
        // Local start dates whose summaries this run invalidates. Sessions are
        // bucketed by their start date/hour (see `end_session`), so that is the
        // only thing a retroactive close can change.
        let mut affected_dates: HashSet<String> = HashSet::new();

        // One transaction for the whole job. Closing the sessions and bringing
        // the summaries in line with them must not be separable: an interruption
        // in between would leave sessions that are listed but counted nowhere,
        // which is precisely the bug this path used to have.
        let tx = rusqlite::Transaction::new_unchecked(
            &self.conn,
            rusqlite::TransactionBehavior::Immediate,
        )?;

        for (id, started_at) in orphans {
            let started = chrono::DateTime::parse_from_rfc3339(&started_at)
                .map(|d| d.with_timezone(&chrono::Utc))
                .unwrap_or(now);

            // Last observed activity for this session, if any.
            let last_event: Option<String> = self
                .conn
                .query_row(
                    "SELECT MAX(COALESCE(ended_at, started_at))
                     FROM activity_events WHERE session_id = ?1",
                    params![id],
                    |row| row.get(0),
                )
                .ok()
                .flatten();

            let mut ended = match last_event {
                Some(s) => chrono::DateTime::parse_from_rfc3339(&s)
                    .map(|d| d.with_timezone(&chrono::Utc))
                    .unwrap_or(started),
                // No events at all: keep the session as a zero-length record
                // rather than dropping it.
                None => started,
            };
            // A stepped clock could put the event in the future; never let a
            // session end before it started or after "now".
            if ended > now {
                ended = now;
            }
            if ended < started {
                ended = started;
            }

            let duration_ms = (ended - started).num_milliseconds().max(0);
            // Same rule as `end_session` / `update_ongoing_focused_ms`, so a
            // session closed here is indistinguishable from one closed in flight.
            let focused_ms = std::cmp::max(0, duration_ms - self.focused_threshold_ms);
            // Sessions are bucketed by their LOCAL start date (see `end_session`).
            let start_date = started
                .with_timezone(&chrono::Local)
                .format("%Y-%m-%d")
                .to_string();

            tx.execute(
                "UPDATE sessions SET ended_at = ?1, duration_ms = ?2, focused_ms = ?3, date_local = ?4 WHERE id = ?5",
                params![ended.to_rfc3339(), duration_ms, focused_ms, start_date, id],
            )?;
            affected_dates.insert(start_date);
            count += 1;
        }

        if !affected_dates.is_empty() {
            self.write_summaries(&tx, Some(&affected_dates))?;
        }
        tx.commit()?;

        if !affected_dates.is_empty() {
            log::info!(
                "Rebuilt summaries for {} day(s) affected by finalised sessions",
                affected_dates.len()
            );
        }

        // No session is open any more; drop their monotonic start markers too.
        // After the commit, so a rolled back transaction leaves them resumable.
        if let Ok(mut starts) = self.monotonic_starts.lock() {
            starts.clear();
        }
        Ok(count)
    }
}
