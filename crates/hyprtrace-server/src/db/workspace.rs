use crate::models::WorkspaceRecommendation;
use rusqlite::params;

impl super::Database {
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
}
