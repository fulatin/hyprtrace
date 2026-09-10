use crate::insights::{
    AnomaliesResponse, CooccurrenceResponse, DisruptionDay, DisruptionImpactResponse,
    ForecastResponse, FragmentationResponse, InsightSession, RhythmResponse, TransitionsResponse,
};
use rusqlite::params;
use std::collections::HashMap;

impl super::Database {
    // -----------------------------------------------------------------------
    // /api/insights/* — read-only analytics. The SQL stays here; every formula
    // lives in `crate::insights` as a pure, unit-tested helper.
    // -----------------------------------------------------------------------

    /// Sessions in the inclusive local-date range, ordered chronologically so
    /// callers can walk consecutive pairs as the app-switch sequence.
    /// Also used by `db::efficiency`, which derives focus time from the sessions
    /// instead of reading the legacy summary column.
    pub(super) fn insight_sessions(
        &self,
        from: &str,
        to: &str,
        min_ms: i64,
    ) -> anyhow::Result<Vec<InsightSession>> {
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
