use rusqlite::params;

impl super::Database {
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
}
