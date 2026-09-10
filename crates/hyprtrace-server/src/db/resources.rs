use crate::models::{AppResource, DisruptionEvent};
use rusqlite::params;

impl super::Database {
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
}
