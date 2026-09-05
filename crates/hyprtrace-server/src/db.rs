use crate::models::{
    ActivityEvent, AiMessage, AppRank, AppResource, CategoryRule, CurrentStatus, DailyActivity,
    DailyTrend, DisruptionEvent, EfficiencyScore, Goal, GoalProgress, HourlyBucket, Project,
    ProjectRule, ProjectStat, Session, TitleStat, TodaySummary, TrendPrediction,
    WorkspaceRecommendation,
};
use anyhow::Context;
use chrono::Timelike;
use rusqlite::{params, Connection};
use std::path::Path;

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

    /// Create the app_categories table (if missing) and seed default rules.
    pub fn ensure_categories(&self) -> anyhow::Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS app_categories (
                id       INTEGER PRIMARY KEY AUTOINCREMENT,
                pattern  TEXT NOT NULL UNIQUE,
                category TEXT NOT NULL,
                priority INTEGER NOT NULL DEFAULT 0
            );",
        )?;

        let defaults: &[(&str, &str)] = &[
            ("code", "development"),
            ("code-insiders", "development"),
            ("codium", "development"),
            ("kitty", "development"),
            ("alacritty", "development"),
            ("wezterm", "development"),
            ("ghostty", "development"),
            ("foot", "development"),
            ("vim", "development"),
            ("nvim", "development"),
            ("emacs", "development"),
            ("idea%", "development"),
            ("pycharm%", "development"),
            ("webstorm%", "development"),
            ("goland%", "development"),
            ("clion%", "development"),
            ("firefox", "browsing"),
            ("chromium", "browsing"),
            ("google-chrome%", "browsing"),
            ("microsoft-edge%", "browsing"),
            ("brave-browser", "browsing"),
            ("zen", "browsing"),
            ("minecraft%", "gaming"),
            ("steam%", "gaming"),
            ("lutris", "gaming"),
            ("heroic", "gaming"),
            ("qq%", "social"),
            ("wechat%", "social"),
            ("telegram%", "social"),
            ("discord", "social"),
            ("mihomo%", "utility"),
            ("kdesystemsettings", "system"),
            ("systemsettings", "system"),
            ("gnome-control-center", "system"),
            ("dolphin", "system"),
            ("nautilus", "system"),
            ("vlc", "media"),
            ("mpv", "media"),
            ("kdenlive", "media"),
            ("obs", "media"),
            ("spotify", "media"),
            ("zathura", "productivity"),
            ("obsidian", "productivity"),
            ("logseq", "productivity"),
            ("thunderbird", "productivity"),
            ("evolution", "productivity"),
        ];
        let mut stmt = self.conn.prepare(
            "INSERT OR IGNORE INTO app_categories (pattern, category, priority) VALUES (?1, ?2, 0)",
        )?;
        for (pattern, category) in defaults {
            stmt.execute(params![pattern, category])?;
        }
        Ok(())
    }

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

    /// All category rules, highest priority first.
    pub fn categories(&self) -> anyhow::Result<Vec<CategoryRule>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, pattern, category, priority FROM app_categories
             ORDER BY priority DESC, id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(CategoryRule {
                id: Some(row.get(0)?),
                pattern: row.get(1)?,
                category: row.get(2)?,
                priority: row.get(3)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Replace all category rules with the given list.
    pub fn set_categories(&self, rules: &[CategoryRule]) -> anyhow::Result<()> {
        self.conn.execute("DELETE FROM app_categories", [])?;
        let mut stmt = self.conn.prepare(
            "INSERT INTO app_categories (pattern, category, priority) VALUES (?1, ?2, ?3)",
        )?;
        for r in rules {
            stmt.execute(params![r.pattern.trim(), r.category.trim(), r.priority])?;
        }
        Ok(())
    }

    /// Classify an app class by matching rules (SQLite LIKE semantics, case-insensitive).
    pub fn categorize(&self, class: &str) -> String {
        let rules = match self.categories() {
            Ok(r) => r,
            Err(_) => return "other".to_string(),
        };
        for rule in rules {
            if like_match(&rule.pattern, class) {
                return rule.category;
            }
        }
        "other".to_string()
    }

    pub fn known_categories() -> Vec<String> {
        [
            "development",
            "browsing",
            "gaming",
            "social",
            "media",
            "productivity",
            "utility",
            "system",
            "other",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    pub fn today_summary(&self, date: &str) -> anyhow::Result<TodaySummary> {
        let total_active_ms: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(total_ms), 0) FROM daily_summary WHERE date = ?1",
            params![date],
            |row| row.get(0),
        )?;
        let total_focused_ms: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(focused_ms), 0) FROM daily_summary WHERE date = ?1",
            params![date],
            |row| row.get(0),
        )?;

        let app_count: usize = self.conn.query_row(
            "SELECT COUNT(DISTINCT class) FROM daily_summary WHERE date = ?1",
            params![date],
            |row| row.get(0),
        )?;

        let session_count: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(session_count), 0) FROM daily_summary WHERE date = ?1",
            params![date],
            |row| row.get(0),
        )?;

        let top_apps = self.app_ranking(date, date, 5)?;

        let total_idle_ms: i64 = {
            // Idle time = gaps between consecutive sessions where the previous
            // session ended because of an idle/away timeout. Small gaps caused
            // by normal app switches (activity_state = active/focused) are NOT
            // idle. This also avoids the old `span - active` formula, which
            // accumulated every tiny switch gap and could drift with clock
            // corrections or overlapping sessions.
            self.conn
                .query_row(
                    "WITH ordered AS (
                        SELECT started_at,
                               ended_at,
                               activity_state,
                               date_local,
                               LAG(ended_at) OVER (ORDER BY started_at) AS prev_end,
                               LAG(activity_state) OVER (ORDER BY started_at) AS prev_state
                        FROM sessions
                        WHERE ended_at IS NOT NULL
                    )
                    SELECT COALESCE(SUM(
                        CAST((julianday(started_at) - julianday(prev_end)) * 86400000 AS INTEGER)
                    ), 0)
                    FROM ordered
                    WHERE date_local = ?1
                      AND prev_end IS NOT NULL
                      AND started_at > prev_end
                      AND prev_state IN ('idle', 'away')",
                    params![date],
                    |row| row.get(0),
                )
                .unwrap_or(0)
        };

        Ok(TodaySummary {
            date: date.to_string(),
            total_active_ms,
            total_idle_ms,
            total_focused_ms,
            app_count,
            session_count,
            top_apps,
        })
    }

    /// Zero-filled daily activity totals for the trailing `days` days (ending
    /// today, inclusive), ordered oldest → newest. Drives the yearly heatmap.
    pub fn daily_activity(&self, days: i64) -> anyhow::Result<Vec<DailyActivity>> {
        let days = days.clamp(7, 730);
        let today = chrono::Local::now().date_naive();
        let start = today - chrono::Duration::days(days - 1);
        let start_str = start.format("%Y-%m-%d").to_string();
        let end_str = today.format("%Y-%m-%d").to_string();

        let mut stmt = self.conn.prepare(
            "SELECT date,
                    COALESCE(SUM(total_ms), 0),
                    COALESCE(SUM(focused_ms), 0),
                    COALESCE(SUM(session_count), 0)
             FROM daily_summary
             WHERE date BETWEEN ?1 AND ?2
             GROUP BY date",
        )?;
        let rows = stmt.query_map(params![start_str, end_str], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?;

        let mut by_date = std::collections::HashMap::new();
        for r in rows {
            let (date, total_ms, focused_ms, session_count) = r?;
            by_date.insert(date, (total_ms, focused_ms, session_count));
        }

        let mut out = Vec::with_capacity(days as usize);
        let mut cursor = start;
        for _ in 0..days {
            let date = cursor.format("%Y-%m-%d").to_string();
            let (total_ms, focused_ms, session_count) =
                by_date.get(&date).copied().unwrap_or((0, 0, 0));
            out.push(DailyActivity {
                date,
                total_ms,
                focused_ms,
                session_count,
            });
            cursor += chrono::Duration::days(1);
        }
        Ok(out)
    }

    /// Aggregate finished sessions by window title over a date range, optionally
    /// restricted to one app class. Empty titles are ignored. Sorted by time desc.
    pub fn title_stats(
        &self,
        from: &str,
        to: &str,
        class: Option<&str>,
        limit: usize,
    ) -> anyhow::Result<Vec<TitleStat>> {
        let limit = limit.clamp(1, 500);
        let (where_clause, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match class {
            Some(c) => (
                "WHERE date_local BETWEEN ?1 AND ?2
                       AND ended_at IS NOT NULL AND duration_ms > 0
                       AND title <> '' AND lower(class) = lower(?3)"
                    .to_string(),
                vec![
                    Box::new(from.to_string()),
                    Box::new(to.to_string()),
                    Box::new(c.to_string()),
                ],
            ),
            None => (
                "WHERE date_local BETWEEN ?1 AND ?2
                       AND ended_at IS NOT NULL AND duration_ms > 0
                       AND title <> ''"
                    .to_string(),
                vec![Box::new(from.to_string()), Box::new(to.to_string())],
            ),
        };

        let sql = format!(
            "SELECT class,
                    title,
                    SUM(duration_ms) AS total_ms,
                    COUNT(*) AS session_count,
                    MAX(started_at) AS last_used_at
             FROM sessions {}
             GROUP BY class, title
             ORDER BY total_ms DESC
             LIMIT ?{}",
            where_clause,
            params.len() + 1,
        );

        let mut all_params: Vec<Box<dyn rusqlite::types::ToSql>> = params;
        all_params.push(Box::new(limit as i64));
        let all_refs: Vec<&dyn rusqlite::types::ToSql> =
            all_params.iter().map(|b| b.as_ref()).collect();

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(all_refs.as_slice(), |row| {
            Ok(TitleStat {
                class: row.get(0)?,
                title: row.get(1)?,
                total_ms: row.get(2)?,
                session_count: row.get(3)?,
                last_used_at: row.get(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn app_ranking(&self, from: &str, to: &str, limit: usize) -> anyhow::Result<Vec<AppRank>> {
        let mut stmt = self.conn.prepare(
            "SELECT class,
                    SUM(total_ms) as total_ms,
                    SUM(session_count) as sessions,
                    SUM(focused_ms) as focused_ms,
                    SUM(focused_session_count) as focused_sessions
             FROM daily_summary WHERE date BETWEEN ?1 AND ?2
             GROUP BY class ORDER BY total_ms DESC LIMIT ?3",
        )?;

        let rows = stmt.query_map(params![from, to, limit as i64], |row| {
            let class: String = row.get(0)?;
            let total_ms: i64 = row.get(1)?;
            let session_count: i64 = row.get(2)?;
            let focused_ms: i64 = row.get(3)?;
            let focused_session_count: i64 = row.get(4)?;
            Ok((
                class,
                total_ms,
                session_count,
                focused_ms,
                focused_session_count,
            ))
        })?;

        let mut results = Vec::new();
        let mut total_all: i64 = 0;
        let mut raw: Vec<(String, i64, i64, i64, i64)> = Vec::new();

        for r in rows {
            let (class, total_ms, session_count, focused_ms, focused_sessions) = r?;
            total_all += total_ms;
            raw.push((class, total_ms, session_count, focused_ms, focused_sessions));
        }

        for (class, total_ms, session_count, focused_ms, focused_session_count) in raw {
            let percentage = if total_all > 0 {
                (total_ms as f64 / total_all as f64) * 100.0
            } else {
                0.0
            };
            let category = self.categorize(&class);
            results.push(AppRank {
                class,
                total_ms,
                percentage,
                session_count,
                focused_ms,
                focused_session_count,
                category,
            });
        }

        Ok(results)
    }

    /// Aggregate resource samples by app class over a date range.
    pub fn resource_stats(
        &self,
        from: &str,
        to: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<AppResource>> {
        let mut stmt = self.conn.prepare(
            "SELECT class,
                    AVG(cpu_pct) as avg_cpu,
                    MAX(mem_kb) as peak_mem,
                    COUNT(*) as samples
             FROM app_resources
             WHERE date_local BETWEEN ?1 AND ?2
             GROUP BY class
             ORDER BY avg_cpu DESC, peak_mem DESC
             LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![from, to, limit as i64], |row| {
            Ok(AppResource {
                class: row.get(0)?,
                avg_cpu_pct: row.get(1)?,
                peak_mem_kb: row.get(2)?,
                sample_count: row.get(3)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Recent disruption events (notifications + clipboard) over a date range.
    pub fn disruptions(
        &self,
        from: &str,
        to: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<DisruptionEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, kind, app, summary, occurred_at
             FROM disruptions
             WHERE date_local BETWEEN ?1 AND ?2
             ORDER BY occurred_at DESC
             LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![from, to, limit as i64], |row| {
            Ok(DisruptionEvent {
                id: row.get(0)?,
                kind: row.get(1)?,
                app: row.get(2)?,
                summary: row.get(3)?,
                occurred_at: row.get(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Compute a 0-100 efficiency score for a single day.
    ///
    /// Factors (each mapped to a sub-score, summed):
    ///   - Focus ratio (focused_ms / active_ms)                    → up to 40
    ///   - Session fragmentation (avg session length)              → up to 30
    ///   - Late-night usage (23:00-05:59 share of active time)     → up to 15
    ///   - Interruptions (notification count, 0-20)                → up to 15
    pub fn efficiency_score(&self, date: &str) -> anyhow::Result<EfficiencyScore> {
        let total_active_ms: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(SUM(total_ms), 0) FROM daily_summary WHERE date = ?1",
                params![date],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let total_focused_ms: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(SUM(focused_ms), 0) FROM daily_summary WHERE date = ?1",
                params![date],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let session_count: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(SUM(session_count), 0) FROM daily_summary WHERE date = ?1",
                params![date],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Late-night share from hourly_summary (local hours 23, 0..6).
        let late_night_ms: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(SUM(total_ms), 0) FROM hourly_summary
             WHERE date = ?1 AND hour >= 23 OR date = ?1 AND hour <= 5",
                params![date, date],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let disruption_count: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(COUNT(*), 0) FROM disruptions
             WHERE date_local = ?1 AND kind = 'notification'",
                params![date],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let focus_ratio = if total_active_ms > 0 {
            total_focused_ms as f64 / total_active_ms as f64
        } else {
            0.0
        };
        let avg_session_secs = if session_count > 0 {
            total_active_ms as f64 / session_count as f64 / 1000.0
        } else {
            0.0
        };
        let late_night_pct = if total_active_ms > 0 {
            (late_night_ms as f64 / total_active_ms as f64) * 100.0
        } else {
            0.0
        };

        let focus_score = (focus_ratio * 40.0).clamp(0.0, 40.0);
        // Ideal avg session 15-90 min → full fragmentation score.
        let frag_score = if avg_session_secs >= 15.0 * 60.0 && avg_session_secs <= 90.0 * 60.0 {
            30.0
        } else if avg_session_secs < 15.0 * 60.0 {
            // Very fragmented: scale from 0 (0s) to 30 (15min).
            (avg_session_secs / (15.0 * 60.0)) * 30.0
        } else {
            // Long sessions: gently taper.
            30.0_f64 - ((avg_session_secs - 90.0 * 60.0) / (180.0 * 60.0)) * 10.0
        };
        let late_score = (15.0 * (1.0 - late_night_pct / 100.0)).clamp(0.0, 15.0);
        let disruption_score = (15.0 * (1.0 - (disruption_count as f64) / 20.0)).clamp(0.0, 15.0);

        let score =
            (focus_score + frag_score.max(0.0) + late_score + disruption_score).round() as i64;

        Ok(EfficiencyScore {
            date: date.to_string(),
            score: score.clamp(0, 100),
            focus_ratio,
            avg_session_secs,
            late_night_pct,
            disruption_count,
            total_active_ms,
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

    /// Predict today's remaining and tomorrow's usage via linear regression over
    /// the past `window` days of daily active time.
    pub fn predict(&self, window: i64) -> anyhow::Result<TrendPrediction> {
        let window = window.clamp(3, 30);
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let start = (chrono::Local::now() - chrono::Duration::days(window - 1))
            .format("%Y-%m-%d")
            .to_string();

        let mut stmt = self.conn.prepare(
            "SELECT date, COALESCE(SUM(total_ms), 0) FROM daily_summary
             WHERE date BETWEEN ?1 AND ?2 GROUP BY date ORDER BY date",
        )?;
        let rows = stmt.query_map(params![start, today], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?;
        let mut daily: Vec<(String, i64)> = Vec::new();
        for r in rows {
            daily.push(r?);
        }

        let today_ms = daily
            .iter()
            .find(|(d, _)| *d == today)
            .map(|(_, ms)| *ms)
            .unwrap_or(0);

        // Linear regression y = a + b*x over the last `window` days.
        let n = daily.len() as f64;
        let (sum_x, sum_y, sum_xy, sum_xx) = daily.iter().enumerate().fold(
            (0.0f64, 0.0f64, 0.0f64, 0.0f64),
            |(sx, sy, sxy, sxx), (i, (_, y))| {
                let x = i as f64;
                (sx + x, sy + *y as f64, sxy + x * *y as f64, sxx + x * x)
            },
        );
        let (slope, intercept) = if n > 1.0 {
            let denom = n * sum_xx - sum_x * sum_x;
            if denom.abs() > f64::EPSILON {
                let b = (n * sum_xy - sum_x * sum_y) / denom;
                let a = (sum_y - b * sum_x) / n;
                (b, a)
            } else {
                (0.0, if n > 0.0 { sum_y / n } else { 0.0 })
            }
        } else {
            (0.0, if n > 0.0 { sum_y / n } else { 0.0 })
        };

        let daily_avg = if n > 0.0 { (sum_y / n) as i64 } else { 0 };
        // Next day index = n (tomorrow). Clamp predictions to [0, 16h].
        let clamp = |v: f64| (v.round() as i64).clamp(0, 16 * 3600 * 1000);
        let predicted_tomorrow_ms = clamp(intercept + slope * n);
        // Today's predicted full-day value; keep at least what we've recorded.
        let predicted_today_full = clamp(intercept + slope * (n - 1.0));
        let predicted_today_ms = predicted_today_full.max(today_ms);

        Ok(TrendPrediction {
            today_ms,
            predicted_today_ms,
            predicted_tomorrow_ms,
            daily_avg_ms: daily_avg,
            slope,
            window_days: window,
        })
    }

    /// Compact live status for lightweight consumers (e.g. Waybar).
    pub fn current_status(&self) -> anyhow::Result<CurrentStatus> {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();

        let current_app: String = self.conn.query_row(
            "SELECT class FROM sessions WHERE ended_at IS NULL ORDER BY started_at DESC LIMIT 1",
            [],
            |r| r.get(0),
        ).unwrap_or_else(|_| "—".to_string());

        let current_session_min: i64 = self.conn.query_row(
            "SELECT COALESCE(CAST((julianday('now') - julianday(started_at)) * 1440 AS INTEGER), 0)
             FROM sessions WHERE ended_at IS NULL ORDER BY started_at DESC LIMIT 1",
            [],
            |r| r.get(0),
        ).unwrap_or(0);

        let today_ms: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(SUM(total_ms), 0) FROM daily_summary WHERE date = ?1",
                params![today],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let today_focused_ms: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(SUM(focused_ms), 0) FROM daily_summary WHERE date = ?1",
                params![today],
                |r| r.get(0),
            )
            .unwrap_or(0);

        // First enabled goal (target) for a progress bar.
        let (goal_name, goal_ms): (Option<String>, i64) = self
            .conn
            .query_row(
                "SELECT name, daily_target_ms FROM goals WHERE enabled = 1 ORDER BY id LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map(|(n, m): (String, i64)| (Some(n), m))
            .unwrap_or((None, 0));

        let today_pct_goal = if goal_ms > 0 {
            (today_ms as f64 / goal_ms as f64) * 100.0
        } else {
            0.0
        };

        let efficiency_score = self.efficiency_score(&today).ok().map(|e| e.score);

        Ok(CurrentStatus {
            current_app,
            current_session_min,
            today_ms,
            today_focused_ms,
            today_pct_goal,
            goal_name,
            efficiency_score,
        })
    }

    /// Analyze historical sessions to recommend which workspace each app should
    /// live on, based on where the user spends the most time per app.
    pub fn workspace_recommendations(
        &self,
        days: i64,
    ) -> anyhow::Result<Vec<WorkspaceRecommendation>> {
        let from = (chrono::Local::now() - chrono::Duration::days(days))
            .format("%Y-%m-%d")
            .to_string();
        let to = chrono::Local::now().format("%Y-%m-%d").to_string();

        // App × workspace totals over the window.
        let mut stmt = self.conn.prepare(
            "SELECT class, workspace, SUM(duration_ms), COUNT(*)
             FROM sessions
             WHERE ended_at IS NOT NULL AND duration_ms > 0 AND workspace IS NOT NULL
               AND date_local BETWEEN ?1 AND ?2
             GROUP BY class, workspace",
        )?;
        let rows = stmt.query_map(params![from, to], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?;

        // Sum per app first, then per app find the dominant workspace.
        let mut app_total: std::collections::HashMap<String, i64> =
            std::collections::HashMap::new();
        let mut app_ws: std::collections::HashMap<(String, String), (i64, i64)> =
            std::collections::HashMap::new();
        for r in rows {
            let (class, ws, ms, count) = r?;
            if ws.is_empty() {
                continue;
            }
            *app_total.entry(class.clone()).or_insert(0) += ms;
            let e = app_ws.entry((class, ws)).or_insert((0, 0));
            e.0 += ms;
            e.1 += count;
        }

        let mut out = Vec::new();
        for (class, total) in app_total {
            let best = app_ws
                .iter()
                .filter(|((c, _), _)| *c == class)
                .max_by_key(|(_, (ms, _))| *ms)
                .map(|((_, ws), (ms, count))| (ws.clone(), *ms, *count));
            if let Some((ws, ws_ms, count)) = best {
                let pct = if total > 0 {
                    ws_ms as f64 / total as f64 * 100.0
                } else {
                    0.0
                };
                let confidence = if pct >= 70.0 {
                    "high".to_string()
                } else if pct >= 40.0 {
                    "medium".to_string()
                } else {
                    "low".to_string()
                };
                out.push(WorkspaceRecommendation {
                    app: class,
                    workspace: ws,
                    time_pct: pct,
                    session_count: count,
                    total_ms: ws_ms,
                    confidence,
                });
            }
        }

        out.sort_by(|a, b| b.total_ms.cmp(&a.total_ms));
        Ok(out)
    }

    pub fn set_goals(&self, goals: &[Goal]) -> anyhow::Result<()> {
        self.conn.execute("DELETE FROM goals", [])?;
        let mut stmt = self.conn.prepare(
            "INSERT INTO goals (name, target_type, target_key, daily_target_ms, enabled)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for g in goals {
            stmt.execute(params![
                g.name.trim(),
                g.target_type,
                g.target_key.as_deref(),
                g.daily_target_ms,
                if g.enabled { 1 } else { 0 },
            ])?;
        }
        Ok(())
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

    pub fn hourly_breakdown(&self, date: &str) -> anyhow::Result<Vec<HourlyBucket>> {
        // Fast path: pre-aggregated hourly_summary (maintained by daemon + rebuild endpoint).
        // Only trust it if it actually has rows for this date — otherwise fall back
        // to computing from raw sessions so fresh installs/old data stay correct.
        let mut hs = self.conn.prepare(
            "SELECT hour, COALESCE(SUM(total_ms), 0), COALESCE(SUM(session_count), 0), COALESCE(SUM(focused_ms), 0)
             FROM hourly_summary WHERE date = ?1
             GROUP BY hour",
        )?;
        let rows = hs.query_map(params![date], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?;

        let mut hs_map = std::collections::HashMap::new();
        for r in rows {
            let (hour, total_ms, sc, focused_ms) = r?;
            hs_map.insert(hour as u8, (total_ms, sc, focused_ms));
        }

        if !hs_map.is_empty() {
            let mut results = Vec::with_capacity(24);
            for h in 0..24u8 {
                let (total_ms, sc, focused_ms) = hs_map.get(&h).copied().unwrap_or((0, 0, 0));
                results.push(HourlyBucket {
                    hour: h,
                    total_ms,
                    session_count: sc,
                    focused_ms,
                });
            }
            return Ok(results);
        }

        let mut stmt = self.conn.prepare(
            "SELECT started_at, duration_ms, COALESCE(focused_ms, 0)
             FROM sessions WHERE date_local = ?1 AND ended_at IS NOT NULL",
        )?;

        let rows = stmt.query_map(params![date], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;

        let mut map = std::collections::HashMap::new();
        for r in rows {
            let (started_at, duration_ms, focused_ms) = r?;
            if let Ok(utc_dt) = chrono::DateTime::parse_from_rfc3339(&started_at) {
                let local_hour = utc_dt.with_timezone(&chrono::Local).hour() as u8;
                let entry = map.entry(local_hour).or_insert((0i64, 0i64, 0i64));
                entry.0 += duration_ms;
                entry.1 += 1;
                entry.2 += focused_ms;
            }
        }

        let mut results = Vec::with_capacity(24);
        for h in 0..24u8 {
            let (total_ms, session_count, focused_ms) = map.get(&h).copied().unwrap_or((0, 0, 0));
            results.push(HourlyBucket {
                hour: h,
                total_ms,
                session_count,
                focused_ms,
            });
        }

        Ok(results)
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
}

/// SQLite LIKE-style pattern match: `%` = any sequence, `_` = one char.
/// Case-insensitive. Used for app-category rules.
fn like_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star, mut mark) = (usize::MAX, 0usize);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == '_' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '%' {
            star = pi;
            mark = ti;
            pi += 1;
        } else if star != usize::MAX {
            pi = star + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '%' {
        pi += 1;
    }
    pi == p.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

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

    /// Open a throwaway in-memory DB and create the tables the delete paths
    /// touch (mirroring the daemon's schema subset we care about here).
    fn test_db() -> Database {
        let db = Database {
            conn: Connection::open_in_memory().unwrap(),
            focused_threshold_ms: 20 * 60 * 1000,
        };
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
                    activity_state TEXT,
                    focused_ms  INTEGER,
                    date_local  TEXT DEFAULT ''
                );
                CREATE TABLE app_resources (
                    id         INTEGER PRIMARY KEY AUTOINCREMENT,
                    session_id INTEGER,
                    class      TEXT NOT NULL,
                    sampled_at TEXT NOT NULL,
                    cpu_pct    REAL,
                    mem_kb     INTEGER,
                    date_local TEXT DEFAULT '',
                    FOREIGN KEY (session_id) REFERENCES sessions(id)
                );
                CREATE TABLE activity_events (
                    id          INTEGER PRIMARY KEY AUTOINCREMENT,
                    session_id  INTEGER,
                    state       TEXT NOT NULL,
                    started_at  TEXT NOT NULL,
                    ended_at    TEXT,
                    duration_ms INTEGER DEFAULT 0,
                    date_local  TEXT DEFAULT '',
                    FOREIGN KEY (session_id) REFERENCES sessions(id)
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
                PRAGMA foreign_keys = ON;",
            )
            .unwrap();
        db
    }

    fn insert_session(db: &Database, class: &str, started_at: &str, ended_at: Option<&str>) -> i64 {
        // date_local is set to the date portion of started_at so the tests are
        // deterministic regardless of the host's timezone.
        let date_local = started_at.get(..10).unwrap_or("").to_string();
        db.conn
            .execute(
                "INSERT INTO sessions (class, title, started_at, ended_at, duration_ms, date_local)
                 VALUES (?1, '', ?2, ?3, 1000, ?4)",
                params![class, started_at, ended_at, date_local],
            )
            .unwrap();
        db.conn.last_insert_rowid()
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

    fn project_test_db() -> Database {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "PRAGMA foreign_keys=ON;
             CREATE TABLE IF NOT EXISTS projects (
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
             );
             CREATE TABLE IF NOT EXISTS sessions (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                class        TEXT NOT NULL,
                title        TEXT NOT NULL DEFAULT '',
                workspace    TEXT,
                started_at   TEXT NOT NULL,
                ended_at     TEXT,
                duration_ms  INTEGER,
                activity_state TEXT,
                focused_ms   INTEGER,
                date_local   TEXT DEFAULT ''
             );",
        )
        .unwrap();
        Database {
            conn,
            focused_threshold_ms: 30_000,
        }
    }

    fn tracked_session(conn: &Connection, class: &str, ms: i64) {
        conn.execute(
            "INSERT INTO sessions (class, title, started_at, ended_at, duration_ms, date_local)
             VALUES (?1, '', '2025-01-01T00:00:00Z', '2025-01-01T01:00:00Z', ?2, '2025-01-01')",
            params![class, ms],
        )
        .unwrap();
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
}
