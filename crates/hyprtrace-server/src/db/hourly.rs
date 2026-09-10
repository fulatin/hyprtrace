use crate::models::HourlyBucket;
use chrono::Timelike;
use rusqlite::params;

impl super::Database {
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
}
