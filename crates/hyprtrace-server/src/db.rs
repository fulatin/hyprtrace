use crate::models::{
    ActivityEvent, AiMessage, AppRank, CategoryRule, DailyTrend, HourlyBucket, Session,
    TodaySummary,
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
            let span_ms: i64 = self
                .conn
                .query_row(
                    "SELECT COALESCE(
                        CAST((julianday(MAX(ended_at)) - julianday(MIN(started_at))) * 86400000 AS INTEGER),
                        0
                    ) FROM sessions WHERE date(started_at) = ?1",
                    params![date],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            std::cmp::max(0, span_ms - total_active_ms)
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
            Ok((class, total_ms, session_count, focused_ms, focused_session_count))
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
             FROM sessions WHERE date(started_at) = ?1 AND ended_at IS NOT NULL",
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
                    "WHERE date(started_at) BETWEEN ?1 AND ?2 AND class = ?3".to_string(),
                    vec![
                        Box::new(from.to_string()),
                        Box::new(to.to_string()),
                        Box::new(class.to_string()),
                    ],
                )
            } else {
                (
                    "WHERE date(started_at) BETWEEN ?1 AND ?2".to_string(),
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
             WHERE date(started_at) BETWEEN ?1 AND ?2 AND ended_at IS NOT NULL
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

    pub fn app_trend_hourly(
        &self,
        class: &str,
        date: &str,
    ) -> anyhow::Result<Vec<DailyTrend>> {
        let mut stmt = self.conn.prepare(
            "SELECT started_at, duration_ms, COALESCE(focused_ms, 0)
             FROM sessions
             WHERE class = ?1 AND date(started_at) = ?2 AND ended_at IS NOT NULL",
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
            let (total_ms, session_count, focused_ms) = buckets.get(&h).copied().unwrap_or((0, 0, 0));
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
             WHERE date(started_at) BETWEEN ?1 AND ?2
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

    pub fn update_ai_message(
        &self,
        id: i64,
        content: &str,
        complete: bool,
    ) -> anyhow::Result<()> {
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

    pub fn rebuild_daily_summary(&self) -> anyhow::Result<()> {
        self.conn.execute("DELETE FROM daily_summary", [])?;
        let threshold = self.focused_threshold_ms;
        let sql = format!(
            "INSERT INTO daily_summary (date, class, total_ms, session_count, focused_ms, focused_session_count)
             SELECT date(started_at) as date,
                    class,
                    SUM(duration_ms) as total_ms,
                    COUNT(*) as session_count,
                    SUM(MAX(0, duration_ms - {})) as focused_ms,
                    SUM(CASE WHEN duration_ms >= {} THEN 1 ELSE 0 END) as focused_sessions
             FROM sessions
             WHERE ended_at IS NOT NULL AND duration_ms > 0
             GROUP BY date(started_at), class",
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
            "SELECT started_at, class, duration_ms, COALESCE(focused_ms, 0)
             FROM sessions
             WHERE ended_at IS NOT NULL AND duration_ms > 0",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?;

        let mut agg: std::collections::HashMap<(String, u8, String), (i64, i64, i64)> =
            std::collections::HashMap::new();
        for r in rows {
            let (started_at, class, duration_ms, focused_ms) = r?;
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&started_at) {
                let date = started_at[..10].to_string();
                let hour = dt.with_timezone(&chrono::Local).hour() as u8;
                let entry = agg.entry((date, hour, class)).or_insert((0, 0, 0));
                entry.0 += duration_ms;
                entry.1 += 1;
                entry.2 += focused_ms;
            }
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
