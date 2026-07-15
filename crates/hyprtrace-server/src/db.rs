use crate::models::{AiMessage, AppRank, DailyTrend, HourlyBucket, Session, TodaySummary};
use anyhow::Context;
use chrono::Timelike;
use rusqlite::{params, Connection};
use std::path::Path;

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("Failed to open database: {:?}", path))?;

        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA busy_timeout=5000;
             PRAGMA foreign_keys=ON;",
        )?;

        Ok(Self { conn })
    }

    pub fn today_summary(&self, date: &str) -> anyhow::Result<TodaySummary> {
        let total_active_ms: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(total_ms), 0) FROM daily_summary WHERE date = ?1",
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
            let span_ms: i64 = self.conn.query_row(
                "SELECT COALESCE(
                    CAST((julianday(MAX(ended_at)) - julianday(MIN(started_at))) * 86400000 AS INTEGER),
                    0
                ) FROM sessions WHERE date(started_at) = ?1",
                params![date],
                |row| row.get(0),
            ).unwrap_or(0);
            std::cmp::max(0, span_ms - total_active_ms)
        };

        Ok(TodaySummary {
            date: date.to_string(),
            total_active_ms,
            total_idle_ms,
            app_count,
            session_count,
            top_apps,
        })
    }

    pub fn app_ranking(&self, from: &str, to: &str, limit: usize) -> anyhow::Result<Vec<AppRank>> {
        let mut stmt = self.conn.prepare(
            "SELECT class, SUM(total_ms) as total_ms, SUM(session_count) as sessions
             FROM daily_summary WHERE date BETWEEN ?1 AND ?2
             GROUP BY class ORDER BY total_ms DESC LIMIT ?3",
        )?;

        let rows = stmt.query_map(params![from, to, limit as i64], |row| {
            let class: String = row.get(0)?;
            let total_ms: i64 = row.get(1)?;
            let session_count: i64 = row.get(2)?;
            Ok((class, total_ms, session_count))
        })?;

        let mut results = Vec::new();
        let mut total_all: i64 = 0;
        let mut raw: Vec<(String, i64, i64)> = Vec::new();

        for r in rows {
            let (class, total_ms, session_count) = r?;
            total_all += total_ms;
            raw.push((class, total_ms, session_count));
        }

        for (class, total_ms, session_count) in raw {
            let percentage = if total_all > 0 {
                (total_ms as f64 / total_all as f64) * 100.0
            } else {
                0.0
            };
            results.push(AppRank {
                class,
                total_ms,
                percentage,
                session_count,
            });
        }

        Ok(results)
    }

    pub fn hourly_breakdown(&self, date: &str) -> anyhow::Result<Vec<HourlyBucket>> {
        let mut stmt = self.conn.prepare(
            "SELECT started_at, duration_ms
             FROM sessions WHERE date(started_at) = ?1 AND ended_at IS NOT NULL",
        )?;

        let rows = stmt.query_map(params![date], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
            ))
        })?;

        let mut map = std::collections::HashMap::new();
        for r in rows {
            let (started_at, duration_ms) = r?;
            // Parse UTC timestamp and convert to local time
            if let Ok(utc_dt) = chrono::DateTime::parse_from_rfc3339(&started_at) {
                let local_hour = utc_dt.with_timezone(&chrono::Local).hour() as u8;
                let entry = map.entry(local_hour).or_insert((0i64, 0i64));
                entry.0 += duration_ms;
                entry.1 += 1;
            }
        }

        let mut results = Vec::with_capacity(24);
        for h in 0..24u8 {
            let (total_ms, session_count) = map.get(&h).copied().unwrap_or((0, 0));
            results.push(HourlyBucket {
                hour: h,
                total_ms,
                session_count,
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
                    format!(
                        "WHERE date(started_at) BETWEEN ?1 AND ?2 AND class = ?3"
                    ),
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
            "SELECT id, class, title, workspace, started_at, ended_at, duration_ms
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
            Ok(Session {
                id: row.get(0)?,
                class: row.get(1)?,
                title: row.get(2)?,
                workspace: row.get(3)?,
                started_at: row.get(4)?,
                ended_at: row.get(5)?,
                duration_ms: row.get(6)?,
            })
        })?;

        let mut sessions = Vec::new();
        for r in rows {
            sessions.push(r?);
        }

        Ok((sessions, total))
    }

    pub fn app_daily_trend(
        &self,
        class: &str,
        from: &str,
        to: &str,
    ) -> anyhow::Result<Vec<DailyTrend>> {
        let mut stmt = self.conn.prepare(
            "SELECT date, total_ms, session_count FROM daily_summary
             WHERE class = ?1 AND date BETWEEN ?2 AND ?3 ORDER BY date",
        )?;

        let rows = stmt.query_map(params![class, from, to], |row| {
            Ok(DailyTrend {
                date: row.get(0)?,
                total_ms: row.get(1)?,
                session_count: row.get(2)?,
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

    pub fn ai_conversations(&self, limit: usize) -> anyhow::Result<Vec<AiMessage>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, created_at, role, content, model FROM ai_conversations
             ORDER BY created_at DESC LIMIT ?1",
        )?;

        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(AiMessage {
                id: row.get(0)?,
                created_at: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                model: row.get(4)?,
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
}