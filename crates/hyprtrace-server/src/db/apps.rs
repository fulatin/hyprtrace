use crate::models::{AppRank, TitleStat};
use rusqlite::params;

impl super::Database {
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
        // SQL `LIMIT` takes a signed value and SQLite reads a negative one as
        // "no limit", so an unclamped `usize::MAX` from a query parameter would
        // cast to -1 and return every row ever recorded.
        let limit = limit.clamp(1, 500);
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
}
