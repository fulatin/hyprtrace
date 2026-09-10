use crate::insights::{
    AnomaliesResponse, CooccurrenceResponse, DisruptionDay, DisruptionImpactResponse,
    ForecastResponse, FragmentationResponse, InsightSession, RhythmResponse, TransitionsResponse,
};
use crate::models::{ActivityEvent, AiMessage, DailyTrend, Goal, GoalProgress, Session};
use anyhow::Context;
use chrono::Timelike;
use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::path::Path;

mod apps;
mod categories;
mod efficiency;
mod hourly;
mod like;
mod migrate;
mod predict;
mod projects;
mod resources;
mod status;
mod summaries;
mod workspace;

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;

pub struct Database {
    conn: Connection,
    focused_threshold_ms: i64,
}

impl Database {
    pub fn open(path: &Path, focused_threshold_seconds: u64) -> anyhow::Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("Failed to open database: {:?}", path))?;

        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA busy_timeout=5000;
             PRAGMA foreign_keys=ON;",
        )?;

        Ok(Self {
            conn,
            focused_threshold_ms: (focused_threshold_seconds * 1000) as i64,
        })
    }
    pub fn goals(&self) -> anyhow::Result<Vec<Goal>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, target_type, target_key, daily_target_ms, enabled FROM goals",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Goal {
                id: Some(row.get(0)?),
                name: row.get(1)?,
                target_type: row.get(2)?,
                target_key: row.get(3)?,
                daily_target_ms: row.get(4)?,
                enabled: row.get::<_, i64>(5)? != 0,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
    /// Persist goals, either replacing the whole table or merging by scope.
    ///
    /// `replace_all = true` keeps the historical "the caller owns the full
    /// list" behaviour, which is what the REST `PUT /goals` endpoint wants: the
    /// web UI reads the list, edits it and posts it back, so a full replace is
    /// the correct, idempotent semantic there.
    ///
    /// `replace_all = false` merges. A goal is matched by `id` when the caller
    /// supplies one, and otherwise by its scope
    /// `(target_type, COALESCE(target_key, ''))`; only if neither matches is a
    /// new row inserted. This matters for the AI tools, where `set_goal` used to
    /// `DELETE FROM goals` first: a single hallucinated or mis-parsed tool call
    /// would silently wipe every goal the user had configured, even though the
    /// model had only asked to change one. An empty and a `NULL` `target_key`
    /// are treated as the same scope because the UI can emit either.
    pub fn set_goals(&self, goals: &[Goal], replace_all: bool) -> anyhow::Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        {
            if replace_all {
                tx.execute("DELETE FROM goals", [])?;
            }
            let mut by_id = tx.prepare(
                "UPDATE goals SET name = ?1, target_type = ?2, target_key = ?3,
                        daily_target_ms = ?4, enabled = ?5
                 WHERE id = ?6",
            )?;
            let mut by_scope = tx.prepare(
                "UPDATE goals SET name = ?1, daily_target_ms = ?2, enabled = ?3
                 WHERE target_type = ?4 AND COALESCE(target_key, '') = COALESCE(?5, '')",
            )?;
            let mut insert = tx.prepare(
                "INSERT INTO goals (name, target_type, target_key, daily_target_ms, enabled)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for g in goals {
                let name = g.name.trim();
                let key = g
                    .target_key
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                let enabled = if g.enabled { 1 } else { 0 };
                // An id from the caller is authoritative; a stale one (goal
                // deleted meanwhile) falls through to the scope match.
                let mut stored = false;
                if let Some(id) = g.id {
                    stored = by_id.execute(params![
                        name,
                        g.target_type,
                        key,
                        g.daily_target_ms,
                        enabled,
                        id
                    ])? > 0;
                }
                if !stored {
                    stored = by_scope.execute(params![
                        name,
                        g.daily_target_ms,
                        enabled,
                        g.target_type,
                        key
                    ])? > 0;
                }
                if !stored {
                    insert.execute(params![
                        name,
                        g.target_type,
                        key,
                        g.daily_target_ms,
                        enabled
                    ])?;
                }
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Delete a single goal, by id when given, otherwise by scope.
    ///
    /// Deleting a goal used to be an implicit side effect of a full replace. It
    /// is a separate operation now so that no caller can drop a goal without
    /// naming it.
    #[allow(dead_code)]
    pub fn delete_goal(
        &self,
        id: Option<i64>,
        target_type: Option<&str>,
        target_key: Option<&str>,
    ) -> anyhow::Result<usize> {
        if let Some(id) = id {
            return Ok(self
                .conn
                .execute("DELETE FROM goals WHERE id = ?1", params![id])?);
        }
        let Some(target_type) = target_type else {
            return Ok(0);
        };
        let key = target_key.map(str::trim).filter(|s| !s.is_empty());
        Ok(self.conn.execute(
            "DELETE FROM goals
             WHERE target_type = ?1 AND COALESCE(target_key, '') = COALESCE(?2, '')",
            params![target_type, key],
        )?)
    }

    /// Compute today's progress toward each goal.
    pub fn goal_progress(&self) -> anyhow::Result<Vec<GoalProgress>> {
        let goals = self.goals()?;
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let mut out = Vec::new();
        for goal in goals {
            let today_ms = if goal.target_type == "class" {
                self.conn
                    .query_row(
                        "SELECT COALESCE(SUM(total_ms), 0) FROM daily_summary WHERE date = ?1 AND class = ?2",
                        params![today, goal.target_key.as_deref().unwrap_or("")],
                        |row| row.get::<_, i64>(0),
                    )
                    .unwrap_or(0)
            } else {
                self.conn
                    .query_row(
                        "SELECT COALESCE(SUM(total_ms), 0) FROM daily_summary WHERE date = ?1",
                        params![today],
                        |row| row.get::<_, i64>(0),
                    )
                    .unwrap_or(0)
            };
            let pct = if goal.daily_target_ms > 0 {
                (today_ms as f64 / goal.daily_target_ms as f64) * 100.0
            } else {
                0.0
            };
            out.push(GoalProgress {
                goal,
                today_ms,
                pct,
            });
        }
        Ok(out)
    }
    /// Build a Markdown usage report for a date range.
    pub fn report(&self, from: &str, to: &str) -> anyhow::Result<String> {
        let mut out = String::new();
        out.push_str(&format!("# HyprTrace Usage Report\n\n"));
        out.push_str(&format!("**Period:** {} → {}\n\n", from, to));

        // Daily totals.
        let total_ms: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(total_ms), 0) FROM daily_summary WHERE date BETWEEN ?1 AND ?2",
            params![from, to],
            |r| r.get(0),
        )?;
        let focused_ms: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(focused_ms), 0) FROM daily_summary WHERE date BETWEEN ?1 AND ?2",
            params![from, to],
            |r| r.get(0),
        )?;
        let sessions: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(session_count), 0) FROM daily_summary WHERE date BETWEEN ?1 AND ?2",
            params![from, to],
            |r| r.get(0),
        )?;
        let hours = total_ms as f64 / 3_600_000.0;
        out.push_str(&format!("**Total active time:** {:.1}h\n", hours));
        out.push_str(&format!(
            "**Focused time:** {:.1}h ({:.0}%)\n",
            focused_ms as f64 / 3_600_000.0,
            if total_ms > 0 {
                focused_ms as f64 / total_ms as f64 * 100.0
            } else {
                0.0
            }
        ));
        out.push_str(&format!("**Sessions:** {}\n", sessions));
        if total_ms > 0 {
            out.push_str(&format!(
                "**Avg session length:** {:.1} min\n",
                total_ms as f64 / sessions.max(1) as f64 / 60_000.0
            ));
        }
        out.push_str("\n");

        // Top apps.
        out.push_str("## Top Apps\n\n");
        out.push_str("| App | Time | % | Focused |\n|---|---|---|---|\n");
        let apps = self.app_ranking(from, to, 15)?;
        for app in apps {
            out.push_str(&format!(
                "| {} | {:.1}h | {:.1}% | {:.1}h |\n",
                app.class,
                app.total_ms as f64 / 3_600_000.0,
                app.percentage,
                app.focused_ms as f64 / 3_600_000.0,
            ));
        }
        out.push_str("\n");

        // Daily breakdown.
        out.push_str("## Daily Breakdown\n\n");
        out.push_str("| Date | Active | Focused | Sessions |\n|---|---|---|---|\n");
        let mut stmt = self.conn.prepare(
            "SELECT date, SUM(total_ms), SUM(focused_ms), SUM(session_count)
             FROM daily_summary WHERE date BETWEEN ?1 AND ?2
             GROUP BY date ORDER BY date",
        )?;
        let rows = stmt.query_map(params![from, to], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?;
        for r in rows {
            let (date, t, f, s) = r?;
            out.push_str(&format!(
                "| {} | {:.1}h | {:.1}h | {} |\n",
                date,
                t as f64 / 3_600_000.0,
                f as f64 / 3_600_000.0,
                s
            ));
        }
        out.push_str("\n");

        // Interruptions.
        let notifications: i64 = self.conn.query_row(
            "SELECT COALESCE(COUNT(*), 0) FROM disruptions
             WHERE date_local BETWEEN ?1 AND ?2 AND kind = 'notification'",
            params![from, to],
            |r| r.get(0),
        )?;
        let copies: i64 = self.conn.query_row(
            "SELECT COALESCE(COUNT(*), 0) FROM disruptions
             WHERE date_local BETWEEN ?1 AND ?2 AND kind = 'clipboard'",
            params![from, to],
            |r| r.get(0),
        )?;
        out.push_str(&format!(
            "## Interruptions\n\nNotifications: {}\n\nClipboard copies: {}\n",
            notifications, copies
        ));

        Ok(out)
    }

    /// Compact weekly totals for the scheduled report notification:
    /// `(total_ms across all classes, top class by total_ms)`.
    pub fn weekly_totals(&self, from: &str, to: &str) -> anyhow::Result<(i64, String)> {
        let total_ms: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(total_ms), 0) FROM daily_summary WHERE date BETWEEN ?1 AND ?2",
            params![from, to],
            |r| r.get(0),
        )?;
        let top_class: String = self.conn.query_row(
            "SELECT COALESCE((SELECT class FROM daily_summary
                 WHERE date BETWEEN ?1 AND ?2
                 GROUP BY class ORDER BY SUM(total_ms) DESC LIMIT 1), '')",
            params![from, to],
            |r| r.get(0),
        )?;
        Ok((total_ms, top_class))
    }
    pub fn sessions_paginated(
        &self,
        from: &str,
        to: &str,
        page: u32,
        per_page: u32,
        class_filter: Option<&str>,
    ) -> anyhow::Result<(Vec<Session>, u32)> {
        let (where_clause, param_values): (String, Vec<Box<dyn rusqlite::types::ToSql>>) =
            if let Some(class) = class_filter {
                (
                    "WHERE date_local BETWEEN ?1 AND ?2 AND class = ?3".to_string(),
                    vec![
                        Box::new(from.to_string()),
                        Box::new(to.to_string()),
                        Box::new(class.to_string()),
                    ],
                )
            } else {
                (
                    "WHERE date_local BETWEEN ?1 AND ?2".to_string(),
                    vec![Box::new(from.to_string()), Box::new(to.to_string())],
                )
            };

        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|b| b.as_ref()).collect();

        let total: u32 = self.conn.query_row(
            &format!("SELECT COUNT(*) FROM sessions {}", where_clause),
            params_refs.as_slice(),
            |row| row.get(0),
        )?;

        let offset = (page.saturating_sub(1)) * per_page;

        let sql = format!(
            "SELECT id, class, title, workspace, started_at, ended_at, duration_ms,
                    activity_state, focused_ms
             FROM sessions {} ORDER BY started_at DESC LIMIT ?{} OFFSET ?{}",
            where_clause,
            param_values.len() + 1,
            param_values.len() + 2,
        );

        let mut all_params: Vec<Box<dyn rusqlite::types::ToSql>> = param_values;
        all_params.push(Box::new(per_page as i64));
        all_params.push(Box::new(offset as i64));

        let all_refs: Vec<&dyn rusqlite::types::ToSql> =
            all_params.iter().map(|b| b.as_ref()).collect();

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(all_refs.as_slice(), |row| {
            let activity_state: Option<String> = row.get(7)?;
            let focused_ms: Option<i64> = row.get(8)?;
            Ok(Session {
                id: row.get(0)?,
                class: row.get(1)?,
                title: row.get(2)?,
                workspace: row.get(3)?,
                started_at: row.get(4)?,
                ended_at: row.get(5)?,
                duration_ms: row.get(6)?,
                activity_state,
                focused_ms,
            })
        })?;

        let mut sessions = Vec::new();
        for r in rows {
            sessions.push(r?);
        }

        Ok((sessions, total))
    }

    pub fn distinct_classes(&self, from: &str, to: &str) -> anyhow::Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT class FROM sessions
             WHERE date_local BETWEEN ?1 AND ?2 AND ended_at IS NOT NULL
             ORDER BY class",
        )?;

        let rows = stmt.query_map(params![from, to], |row| row.get::<_, String>(0))?;

        let mut classes = Vec::new();
        for r in rows {
            classes.push(r?);
        }
        Ok(classes)
    }

    pub fn app_daily_trend(
        &self,
        class: &str,
        from: &str,
        to: &str,
    ) -> anyhow::Result<Vec<DailyTrend>> {
        let mut stmt = self.conn.prepare(
            "SELECT date, total_ms, session_count, focused_ms FROM daily_summary
             WHERE class = ?1 AND date BETWEEN ?2 AND ?3 ORDER BY date",
        )?;

        let rows = stmt.query_map(params![class, from, to], |row| {
            Ok(DailyTrend {
                date: row.get(0)?,
                total_ms: row.get(1)?,
                session_count: row.get(2)?,
                focused_ms: row.get(3)?,
            })
        })?;

        let mut results = Vec::new();
        for r in rows {
            results.push(r?);
        }

        Ok(results)
    }

    pub fn app_trend_hourly(&self, class: &str, date: &str) -> anyhow::Result<Vec<DailyTrend>> {
        let mut stmt = self.conn.prepare(
            "SELECT started_at, duration_ms, COALESCE(focused_ms, 0)
             FROM sessions
             WHERE class = ?1 AND date_local = ?2 AND ended_at IS NOT NULL",
        )?;

        let rows = stmt.query_map(params![class, date], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;

        let mut buckets = std::collections::BTreeMap::new();
        for r in rows {
            let (started_at, duration_ms, focused_ms) = r?;
            if let Ok(utc_dt) = chrono::DateTime::parse_from_rfc3339(&started_at) {
                let local_hour = utc_dt.with_timezone(&chrono::Local).hour() as u8;
                let entry = buckets.entry(local_hour).or_insert((0i64, 0i64, 0i64));
                entry.0 += duration_ms;
                entry.1 += 1;
                entry.2 += focused_ms;
            }
        }

        let mut results = Vec::with_capacity(24);
        for h in 0..24u8 {
            let (total_ms, session_count, focused_ms) =
                buckets.get(&h).copied().unwrap_or((0, 0, 0));
            results.push(DailyTrend {
                date: format!("{:02}:00", h),
                total_ms,
                session_count,
                focused_ms,
            });
        }

        Ok(results)
    }

    pub fn activity_events(
        &self,
        from: &str,
        to: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<ActivityEvent>> {
        // See `app_ranking`: a negative LIMIT means "unlimited" in SQLite.
        let limit = limit.clamp(1, 1000);
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, state, started_at, ended_at, duration_ms
             FROM activity_events
             WHERE date_local BETWEEN ?1 AND ?2
             ORDER BY started_at DESC LIMIT ?3",
        )?;

        let rows = stmt.query_map(params![from, to, limit as i64], |row| {
            Ok(ActivityEvent {
                id: row.get(0)?,
                session_id: row.get(1)?,
                state: row.get(2)?,
                started_at: row.get(3)?,
                ended_at: row.get(4)?,
                duration_ms: row.get(5)?,
            })
        })?;

        let mut results = Vec::new();
        for r in rows {
            results.push(r?);
        }
        Ok(results)
    }
    pub fn save_ai_message(&self, role: &str, content: &str, model: &str) -> anyhow::Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO ai_conversations (created_at, role, content, model) VALUES (?1, ?2, ?3, ?4)",
            params![now, role, content, model],
        )?;
        Ok(())
    }

    pub fn save_ai_message_streaming(
        &self,
        role: &str,
        content: &str,
        model: &str,
    ) -> anyhow::Result<i64> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO ai_conversations (created_at, role, content, model, complete) VALUES (?1, ?2, ?3, ?4, 0)",
            params![now, role, content, model],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_ai_message(&self, id: i64, content: &str, complete: bool) -> anyhow::Result<()> {
        self.conn.execute(
            "UPDATE ai_conversations SET content = ?1, complete = ?2 WHERE id = ?3",
            params![content, if complete { 1 } else { 0 }, id],
        )?;
        Ok(())
    }

    pub fn ai_conversations(&self, limit: usize) -> anyhow::Result<Vec<AiMessage>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, created_at, role, content, model, complete FROM ai_conversations
             ORDER BY created_at DESC LIMIT ?1",
        )?;

        let rows = stmt.query_map(params![limit as i64], |row| {
            let complete: Option<i64> = row.get(5)?;
            Ok(AiMessage {
                id: row.get(0)?,
                created_at: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                model: row.get(4)?,
                complete: complete.map(|v| v != 0),
            })
        })?;

        let mut results = Vec::new();
        for r in rows {
            results.push(r?);
        }
        results.reverse();
        Ok(results)
    }

    pub fn clear_ai_conversations(&self) -> anyhow::Result<()> {
        self.conn.execute("DELETE FROM ai_conversations", [])?;
        Ok(())
    }
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

    pub fn rebuild_daily_summary(&self) -> anyhow::Result<()> {
        self.conn.execute("DELETE FROM daily_summary", [])?;
        let threshold = self.focused_threshold_ms;
        let sql = format!(
            "INSERT INTO daily_summary (date, class, total_ms, session_count, focused_ms, focused_session_count)
             SELECT date_local as date,
                    class,
                    SUM(duration_ms) as total_ms,
                    COUNT(*) as session_count,
                    SUM(MAX(0, duration_ms - {})) as focused_ms,
                    SUM(CASE WHEN duration_ms >= {} THEN 1 ELSE 0 END) as focused_sessions
             FROM sessions
             WHERE ended_at IS NOT NULL AND duration_ms > 0 AND date_local <> ''
             GROUP BY date_local, class",
            threshold, threshold,
        );
        self.conn.execute_batch(&sql)?;
        Ok(())
    }

    pub fn rebuild_hourly_summary(&self) -> anyhow::Result<()> {
        self.conn.execute("DELETE FROM hourly_summary", [])?;

        // Aggregate in Rust so we can bucket by LOCAL hour (chrono::Local),
        // consistent with the sessions-based fallback and the daemon's incremental upsert.
        let mut stmt = self.conn.prepare(
            "SELECT started_at, date_local, class, duration_ms, COALESCE(focused_ms, 0)
             FROM sessions
             WHERE ended_at IS NOT NULL AND duration_ms > 0 AND date_local <> ''",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })?;

        let mut agg: std::collections::HashMap<(String, u8, String), (i64, i64, i64)> =
            std::collections::HashMap::new();
        for r in rows {
            let (started_at, date, class, duration_ms, focused_ms) = r?;
            let hour = chrono::DateTime::parse_from_rfc3339(&started_at)
                .ok()
                .map(|dt| dt.with_timezone(&chrono::Local).hour() as u8)
                .unwrap_or(0);
            let entry = agg.entry((date, hour, class)).or_insert((0, 0, 0));
            entry.0 += duration_ms;
            entry.1 += 1;
            entry.2 += focused_ms;
        }

        for ((date, hour, class), (total_ms, session_count, focused_ms)) in agg {
            self.conn.execute(
                "INSERT INTO hourly_summary (date, hour, class, total_ms, session_count, focused_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![date, hour as i64, class, total_ms, session_count, focused_ms],
            )?;
        }

        Ok(())
    }
    // -----------------------------------------------------------------------
    // /api/insights/* — read-only analytics. The SQL stays here; every formula
    // lives in `crate::insights` as a pure, unit-tested helper.
    // -----------------------------------------------------------------------

    /// Sessions in the inclusive local-date range, ordered chronologically so
    /// callers can walk consecutive pairs as the app-switch sequence.
    fn insight_sessions(&self, from: &str, to: &str, min_ms: i64) -> anyhow::Result<Vec<InsightSession>> {
        let mut stmt = self.conn.prepare(
            "SELECT class, started_at, ended_at, COALESCE(duration_ms, 0), date_local
             FROM sessions
             WHERE date_local BETWEEN ?1 AND ?2 AND COALESCE(duration_ms, 0) >= ?3
             ORDER BY started_at ASC",
        )?;
        let rows = stmt.query_map(params![from, to, min_ms], |row| {
            Ok(InsightSession {
                class: row.get(0)?,
                started_at: row.get(1)?,
                ended_at: row.get::<_, Option<String>>(2)?,
                duration_ms: row.get(3)?,
                date_local: row.get(4)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn insight_transitions(
        &self,
        from: &str,
        to: &str,
        min_ms: i64,
        limit: usize,
    ) -> anyhow::Result<TransitionsResponse> {
        let sessions = self.insight_sessions(from, to, min_ms)?;
        Ok(crate::insights::transitions(
            &sessions, from, to, min_ms, limit,
        ))
    }

    /// Daily totals for the forecast window: one class when `class` is set,
    /// otherwise every class summed per day. Days without rows are zero-filled
    /// by `insights::forecast`, so only present days are returned here.
    pub fn insight_forecast(
        &self,
        class: Option<&str>,
        to: &str,
        days: i64,
        horizon: i64,
    ) -> anyhow::Result<ForecastResponse> {
        let start = chrono::NaiveDate::parse_from_str(to, "%Y-%m-%d")
            .map(|d| {
                (d - chrono::Duration::days((days - 1).max(0)))
                    .format("%Y-%m-%d")
                    .to_string()
            })
            .unwrap_or_else(|_| to.to_string());

        let mut totals: HashMap<String, i64> = HashMap::new();
        match class {
            Some(c) if !c.is_empty() => {
                let mut stmt = self.conn.prepare(
                    "SELECT date, COALESCE(SUM(total_ms), 0) FROM daily_summary
                     WHERE date BETWEEN ?1 AND ?2 AND class = ?3
                     GROUP BY date ORDER BY date",
                )?;
                let rows = stmt.query_map(params![start, to, c], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
                })?;
                for r in rows {
                    let (date, ms) = r?;
                    totals.insert(date, ms);
                }
            }
            _ => {
                let mut stmt = self.conn.prepare(
                    "SELECT date, COALESCE(SUM(total_ms), 0) FROM daily_summary
                     WHERE date BETWEEN ?1 AND ?2
                     GROUP BY date ORDER BY date",
                )?;
                let rows = stmt.query_map(params![start, to], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
                })?;
                for r in rows {
                    let (date, ms) = r?;
                    totals.insert(date, ms);
                }
            }
        }

        Ok(crate::insights::forecast(
            &totals,
            class.unwrap_or(""),
            to,
            days,
            horizon,
        ))
    }

    pub fn insight_rhythm(&self, from: &str, to: &str) -> anyhow::Result<RhythmResponse> {
        let mut stmt = self.conn.prepare(
            "SELECT date, hour, COALESCE(SUM(total_ms), 0) FROM hourly_summary
             WHERE date BETWEEN ?1 AND ?2 AND hour >= 0 AND hour <= 23
             GROUP BY date, hour ORDER BY date, hour",
        )?;
        let rows = stmt.query_map(params![from, to], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)? as u8,
                r.get::<_, i64>(2)?,
            ))
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(crate::insights::rhythm(&out, from, to))
    }

    pub fn insight_fragmentation(
        &self,
        from: &str,
        to: &str,
    ) -> anyhow::Result<FragmentationResponse> {
        let sessions = self.insight_sessions(from, to, 0)?;
        Ok(crate::insights::fragmentation(&sessions, from, to))
    }

    pub fn insight_cooccurrence(
        &self,
        from: &str,
        to: &str,
        window_minutes: i64,
        limit: usize,
    ) -> anyhow::Result<CooccurrenceResponse> {
        let sessions = self.insight_sessions(from, to, 0)?;
        Ok(crate::insights::cooccurrence(
            &sessions,
            from,
            to,
            window_minutes,
            limit,
        ))
    }

    /// Per-day interruption counts, efficiency and fragmentation metrics.
    ///
    /// Only days with activity or disruptions are returned; the efficiency
    /// sub-score is recomputed here (instead of calling `efficiency_score` per
    /// day) so the whole range is one query.
    pub fn insight_disruption_impact(
        &self,
        from: &str,
        to: &str,
    ) -> anyhow::Result<DisruptionImpactResponse> {
        let mut stmt = self.conn.prepare(
            "SELECT u.date,
                    COALESCE(s.active_ms, 0),
                    COALESCE(s.focused_ms, 0),
                    COALESCE(s.session_count, 0),
                    COALESCE(d.total, 0),
                    COALESCE(d.notifications, 0),
                    COALESCE(d.clipboard, 0),
                    COALESCE(h.late_night_ms, 0)
             FROM (
                 SELECT date FROM daily_summary WHERE date BETWEEN ?1 AND ?2
                 UNION
                 SELECT date_local AS date FROM disruptions WHERE date_local BETWEEN ?1 AND ?2
             ) u
             LEFT JOIN (
                 SELECT date, SUM(total_ms) AS active_ms, SUM(focused_ms) AS focused_ms,
                        SUM(session_count) AS session_count
                 FROM daily_summary WHERE date BETWEEN ?1 AND ?2 GROUP BY date
             ) s ON s.date = u.date
             LEFT JOIN (
                 SELECT date_local AS date, COUNT(*) AS total,
                        SUM(CASE WHEN kind = 'notification' THEN 1 ELSE 0 END) AS notifications,
                        SUM(CASE WHEN kind = 'clipboard' THEN 1 ELSE 0 END) AS clipboard
                 FROM disruptions WHERE date_local BETWEEN ?1 AND ?2 GROUP BY date_local
             ) d ON d.date = u.date
             LEFT JOIN (
                 SELECT date, SUM(total_ms) AS late_night_ms FROM hourly_summary
                 WHERE date BETWEEN ?1 AND ?2 AND (hour >= 23 OR hour <= 5) GROUP BY date
             ) h ON h.date = u.date",
        )?;
        let rows = stmt.query_map(params![from, to], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, i64>(7)?,
            ))
        })?;

        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut days = Vec::new();
        for r in rows {
            let (date, active_ms, focused_ms, session_count, total, notifications, clipboard, late) =
                r?;
            if !seen.insert(date.clone()) {
                continue;
            }
            if active_ms <= 0 && total <= 0 {
                continue;
            }
            let efficiency = crate::insights::efficiency_subscore(
                active_ms,
                focused_ms,
                session_count,
                late,
                notifications,
            );
            days.push(DisruptionDay {
                date,
                disruptions: total,
                notifications,
                clipboard,
                efficiency,
                focus_ratio: if active_ms > 0 {
                    focused_ms as f64 / active_ms as f64
                } else {
                    0.0
                },
                active_ms,
                avg_dwell_ms: if session_count > 0 {
                    active_ms / session_count
                } else {
                    0
                },
                session_count,
            });
        }
        Ok(crate::insights::disruption_impact(days, from, to))
    }

    /// Days that deviate from the trailing `window`-day habit.
    ///
    /// `active_ms` comes from `daily_summary` (summed over classes); the other
    /// per-day fields come from `sessions` and `hourly_summary`.
    pub fn insight_anomalies(
        &self,
        from: &str,
        to: &str,
        window: i64,
        threshold: f64,
    ) -> anyhow::Result<AnomaliesResponse> {
        // Fetch `window` extra days before `from` so the first reportable day
        // already has a full baseline.
        let fetch_from = chrono::NaiveDate::parse_from_str(from, "%Y-%m-%d")
            .map(|d| {
                (d - chrono::Duration::days(window.max(0)))
                    .format("%Y-%m-%d")
                    .to_string()
            })
            .unwrap_or_else(|_| from.to_string());

        let mut active: HashMap<String, i64> = HashMap::new();
        let mut stmt = self.conn.prepare(
            "SELECT date, COALESCE(SUM(total_ms), 0) FROM daily_summary
             WHERE date BETWEEN ?1 AND ?2 GROUP BY date",
        )?;
        let rows = stmt.query_map(params![fetch_from, to], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?;
        for r in rows {
            let (date, ms) = r?;
            active.insert(date, ms);
        }

        let mut late: HashMap<String, i64> = HashMap::new();
        let mut stmt = self.conn.prepare(
            "SELECT date, COALESCE(SUM(total_ms), 0) FROM hourly_summary
             WHERE date BETWEEN ?1 AND ?2 AND (hour >= 23 OR hour <= 5) GROUP BY date",
        )?;
        let rows = stmt.query_map(params![fetch_from, to], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?;
        for r in rows {
            let (date, ms) = r?;
            late.insert(date, ms);
        }

        // Per-day session aggregates over the same extended range.
        let sessions = self.insight_sessions(&fetch_from, to, 0)?;
        let mut per_day: HashMap<String, (i64, i64, i64)> = HashMap::new(); // count, total_ms, short
        for s in &sessions {
            let e = per_day.entry(s.date_local.clone()).or_insert((0, 0, 0));
            e.0 += 1;
            e.1 += s.duration_ms.max(0);
            if s.duration_ms < 60_000 {
                e.2 += 1;
            }
        }

        let mut inputs = Vec::new();
        for (date, active_ms) in active.iter() {
            let (count, total_ms, short) = per_day.get(date).copied().unwrap_or((0, 0, 0));
            let late_ms = late.get(date).copied().unwrap_or(0);
            inputs.push(crate::insights::AnomalyInput {
                date: date.clone(),
                active_ms: *active_ms,
                session_count: count,
                avg_dwell_ms: if count > 0 { total_ms / count } else { 0 },
                late_night_share: if *active_ms > 0 {
                    late_ms as f64 / *active_ms as f64
                } else {
                    0.0
                },
                short_session_ratio: if count > 0 {
                    short as f64 / count as f64
                } else {
                    0.0
                },
            });
        }
        // NOTE: late-night hours (23:00 and 00:00-05:59) are filtered in SQL by
        // `insights::is_late_night_hour`'s predicate; keep both in sync.
        Ok(crate::insights::anomalies(
            &inputs, from, to, window, threshold,
        ))
    }
}
