use crate::models::TrendPrediction;
use rusqlite::params;

impl super::Database {
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
}
