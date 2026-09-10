use crate::models::{Project, ProjectRule, ProjectStat};
use rusqlite::params;

use super::like::like_match;

impl super::Database {
    /// Create the projects + project_rules tables (if missing).
    pub fn ensure_projects(&self) -> anyhow::Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS projects (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                name       TEXT NOT NULL UNIQUE,
                color      TEXT NOT NULL DEFAULT '#22d3ee',
                sort_order INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE IF NOT EXISTS project_rules (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                pattern    TEXT NOT NULL,
                priority   INTEGER NOT NULL DEFAULT 0
            );",
        )?;
        Ok(())
    }

    /// All projects, ordered by sort_order.
    pub fn projects(&self) -> anyhow::Result<Vec<Project>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, color, sort_order FROM projects ORDER BY sort_order ASC, id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Project {
                id: Some(row.get(0)?),
                name: row.get(1)?,
                color: row.get(2)?,
                sort_order: row.get(3)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// All project rules, ordered by project then priority.
    pub fn project_rules(&self) -> anyhow::Result<Vec<ProjectRule>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, pattern, priority FROM project_rules
             ORDER BY priority DESC, id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ProjectRule {
                id: Some(row.get(0)?),
                project_id: row.get(1)?,
                pattern: row.get(2)?,
                priority: row.get(3)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Replace all projects and their rules atomically. Rules referencing a
    /// project that no longer exists are removed first so foreign keys hold.
    pub fn set_projects(&self, projects: &[Project], rules: &[ProjectRule]) -> anyhow::Result<()> {
        let tx = self.conn.unchecked_transaction()?;

        // Rebuild projects, tracking old→new id so rules can be reattached.
        tx.execute("DELETE FROM projects", [])?;
        let mut id_map: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
        {
            let mut stmt =
                tx.prepare("INSERT INTO projects (name, color, sort_order) VALUES (?1, ?2, ?3)")?;
            for (idx, p) in projects.iter().enumerate() {
                if p.name.trim().is_empty() {
                    continue;
                }
                let old_id = p.id;
                stmt.execute(params![
                    p.name.trim(),
                    if p.color.trim().is_empty() {
                        "#22d3ee".to_string()
                    } else {
                        p.color.trim().to_string()
                    },
                    idx as i64,
                ])?;
                let new_id = tx.last_insert_rowid();
                if let Some(oid) = old_id {
                    id_map.insert(oid, new_id);
                }
            }
        }

        // Reinsert rules, mapping any old project ids to their new ids.
        {
            let mut rule_stmt = tx.prepare(
                "INSERT INTO project_rules (project_id, pattern, priority) VALUES (?1, ?2, ?3)",
            )?;
            for r in rules {
                let project_id = id_map.get(&r.project_id).copied().unwrap_or(r.project_id);
                // Skip rules whose project no longer exists.
                let exists: i64 = tx.query_row(
                    "SELECT COUNT(*) FROM projects WHERE id = ?1",
                    params![project_id],
                    |row| row.get(0),
                )?;
                if exists == 0 || r.pattern.trim().is_empty() {
                    continue;
                }
                rule_stmt.execute(params![project_id, r.pattern.trim(), r.priority])?;
            }
        }

        tx.commit()?;
        Ok(())
    }

    /// Find the project matching an app class (SQLite LIKE semantics,
    /// case-insensitive), highest priority rule first.
    pub fn project_for_class(&self, class: &str) -> Option<Project> {
        let rules = self.project_rules().ok()?;
        for rule in rules {
            if like_match(&rule.pattern, class) {
                let project = self.conn.query_row(
                    "SELECT id, name, color, sort_order FROM projects WHERE id = ?1",
                    params![rule.project_id],
                    |row| {
                        Ok(Project {
                            id: Some(row.get(0)?),
                            name: row.get(1)?,
                            color: row.get(2)?,
                            sort_order: row.get(3)?,
                        })
                    },
                );
                if let Ok(p) = project {
                    return Some(p);
                }
            }
        }
        None
    }

    /// Aggregate session time by matched project for a date range, including an
    /// "未分类" (uncategorized) bucket for unmatched sessions.
    pub fn project_stats(&self, from: &str, to: &str) -> anyhow::Result<Vec<ProjectStat>> {
        let projects = self.projects()?;

        let mut stmt = self.conn.prepare(
            "SELECT class, SUM(duration_ms), COUNT(*)
             FROM sessions
             WHERE ended_at IS NOT NULL AND duration_ms > 0
               AND date_local BETWEEN ?1 AND ?2
             GROUP BY class",
        )?;
        let rows = stmt.query_map(params![from, to], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;

        // Bucket by project id (None = unmatched).
        let mut buckets: std::collections::HashMap<Option<i64>, (i64, i64)> =
            std::collections::HashMap::new();
        for r in rows {
            let (class, total_ms, session_count) = r?;
            let pid = self.project_for_class(&class).map(|p| p.id.unwrap_or(-1));
            let key = pid.filter(|&id| id > 0);
            let entry = buckets.entry(key).or_insert((0, 0));
            entry.0 += total_ms;
            entry.1 += session_count;
        }

        let grand_total: i64 = buckets.values().map(|(ms, _)| *ms).sum();

        let mut out = Vec::new();
        for p in &projects {
            let pid = p.id.expect("projects loaded from db always have ids");
            let (total_ms, session_count) = buckets.get(&Some(pid)).copied().unwrap_or((0, 0));
            let percentage = if grand_total > 0 {
                (total_ms as f64 / grand_total as f64) * 100.0
            } else {
                0.0
            };
            out.push(ProjectStat {
                project_id: Some(pid),
                name: p.name.clone(),
                color: p.color.clone(),
                total_ms,
                session_count,
                percentage,
            });
        }

        // Unmatched bucket.
        let (unmatched_ms, unmatched_count) = buckets.get(&None).copied().unwrap_or((0, 0));
        if unmatched_ms > 0 {
            out.push(ProjectStat {
                project_id: None,
                name: "未分类".to_string(),
                color: "#6b7280".to_string(),
                total_ms: unmatched_ms,
                session_count: unmatched_count,
                percentage: if grand_total > 0 {
                    (unmatched_ms as f64 / grand_total as f64) * 100.0
                } else {
                    0.0
                },
            });
        }

        // Sort by total time descending so the busiest buckets lead.
        out.sort_by(|a, b| b.total_ms.cmp(&a.total_ms));
        Ok(out)
    }
}
