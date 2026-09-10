use crate::models::{Goal, GoalProgress};
use rusqlite::params;

impl super::Database {
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
}
