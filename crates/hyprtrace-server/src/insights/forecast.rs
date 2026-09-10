use chrono::Duration as ChronoDuration;
use serde::Serialize;
use std::collections::HashMap;

use super::stats::{mean, ols, weekday_factors};
use super::time::{date_shift, parse_date, today_local, weekday_mon0};

// ---------------------------------------------------------------------------
// 2. Forecast
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct ForecastPoint {
    pub date: String,
    pub actual_ms: i64,
    pub predicted_ms: i64,
    pub lower_ms: i64,
    pub upper_ms: i64,
}

#[derive(Debug, Serialize)]
pub struct ForecastFuturePoint {
    pub date: String,
    pub predicted_ms: i64,
    pub lower_ms: i64,
    pub upper_ms: i64,
}

#[derive(Debug, Serialize)]
pub struct ForecastResponse {
    pub class: String,
    pub days: i64,
    pub horizon: i64,
    pub slope_ms_per_day: f64,
    pub intercept_ms: f64,
    pub r2: f64,
    pub mae_ms: f64,
    pub residual_std_ms: f64,
    pub avg_ms: f64,
    pub trend: String,
    pub today_ms: i64,
    pub today_projected_ms: i64,
    pub tomorrow_projected_ms: i64,
    pub weekday_factors: [f64; 7],
    pub points: Vec<ForecastPoint>,
    pub forecast: Vec<ForecastFuturePoint>,
}

/// Fill the last `days` local days with zeros so the regression sees a
/// continuous series (a missing day means "no usage", not "no data point").
fn continuous_series(totals: &HashMap<String, i64>, to: &str, days: i64) -> Vec<(String, i64)> {
    let end = match parse_date(to) {
        Some(d) => d,
        None => return Vec::new(),
    };
    let start = end - ChronoDuration::days((days - 1).max(0));
    let mut out = Vec::with_capacity(days.max(0) as usize);
    let mut cur = start;
    while cur <= end {
        let key = cur.format("%Y-%m-%d").to_string();
        let value = totals.get(&key).copied().unwrap_or(0);
        out.push((key, value));
        cur += ChronoDuration::days(1);
    }
    out
}

/// Daily usage forecast: an OLS trend over the continuous, zero-filled series
/// multiplied by a per-weekday factor.
///
/// The regression intentionally runs on the zero-filled series — a day with no
/// recorded activity is real information ("the PC was off") and including it
/// keeps `slope_ms_per_day` and `r2` honest about the user's actual pattern.
/// The weekday factors, however, are computed from active days only (see
/// `weekday_factors`) so a weekday that happens to contain no activity never
/// projects to zero.
pub fn forecast(
    totals: &HashMap<String, i64>,
    class: &str,
    to: &str,
    days: i64,
    horizon: i64,
) -> ForecastResponse {
    let series = continuous_series(totals, to, days);
    let dates: Vec<String> = series.iter().map(|(d, _)| d.clone()).collect();
    let values: Vec<i64> = series.iter().map(|(_, v)| *v).collect();
    let y: Vec<f64> = values.iter().map(|v| *v as f64).collect();
    let fit = ols(&y);
    let factors = weekday_factors(&dates, &values);
    let avg_ms = mean(&y);

    let interval = 1.96 * fit.residual_std;

    let project = |index: f64, date: &str| -> (i64, i64, i64) {
        let base = fit.intercept + fit.slope * index;
        let factor = parse_date(date)
            .map(|d| factors[weekday_mon0(d) as usize])
            .unwrap_or(1.0);
        let predicted = base * factor;
        let predicted = if predicted.is_finite() { predicted } else { 0.0 };
        let lower = (predicted - interval).max(0.0);
        let upper = (predicted + interval).max(0.0);
        (
            predicted.round().max(0.0) as i64,
            lower.round().max(0.0) as i64,
            upper.round().max(0.0) as i64,
        )
    };

    let mut points = Vec::with_capacity(series.len());
    for (i, (date, actual)) in series.iter().enumerate() {
        let (predicted, lower, upper) = project(i as f64, date);
        points.push(ForecastPoint {
            date: date.clone(),
            actual_ms: *actual,
            predicted_ms: predicted,
            lower_ms: lower,
            upper_ms: upper,
        });
    }

    let n = series.len();
    let mut future = Vec::with_capacity(horizon.max(0) as usize);
    for h in 1..=horizon.max(0) {
        let Some(date) = date_shift(to, h) else { break };
        let (predicted, lower, upper) = project((n as i64 - 1 + h) as f64, &date);
        future.push(ForecastFuturePoint {
            date,
            predicted_ms: predicted,
            lower_ms: lower,
            upper_ms: upper,
        });
    }

    // "Today" is the last day of the series; keep at least what was recorded.
    let today_date = today_local();
    let today_ms = series
        .iter()
        .find(|(d, _)| *d == today_date)
        .map(|(_, v)| *v)
        .unwrap_or(0);
    let today_index = series
        .iter()
        .position(|(d, _)| *d == today_date)
        .map(|i| i as f64)
        .unwrap_or((n as i64 - 1) as f64);
    let (today_projected, _, _) = project(today_index, &today_date);
    let today_projected_ms = today_projected.max(today_ms);
    let tomorrow_projected_ms = future.first().map(|f| f.predicted_ms).unwrap_or(0);

    // Trend is only "real" when the whole-window movement clears 5% of the mean.
    let trend = if avg_ms <= f64::EPSILON {
        "flat"
    } else if fit.slope.abs() * (days as f64) < 0.05 * avg_ms {
        "flat"
    } else if fit.slope > 0.0 {
        "up"
    } else {
        "down"
    };

    ForecastResponse {
        class: class.to_string(),
        days,
        horizon,
        slope_ms_per_day: finite(fit.slope),
        intercept_ms: finite(fit.intercept),
        r2: finite(fit.r2),
        mae_ms: finite(fit.mae),
        residual_std_ms: finite(fit.residual_std),
        avg_ms: finite(avg_ms),
        trend: trend.to_string(),
        today_ms,
        today_projected_ms,
        tomorrow_projected_ms,
        weekday_factors: factors.map(finite),
        points,
        forecast: future,
    }
}

pub(crate) fn finite(v: f64) -> f64 {
    if v.is_finite() {
        v
    } else {
        0.0
    }
}
