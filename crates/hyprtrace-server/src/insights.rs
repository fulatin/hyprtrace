//! Pure analytics helpers and serde models for the read-only `/api/insights/*`
//! endpoints.
//!
//! Everything in this module is a plain function over already-fetched rows so it
//! can be unit-tested without a database (see `mod tests` at the bottom). The
//! SQL lives in `db.rs`; the HTTP plumbing lives in `routes/insights.rs`.
//!
//! Contract notes that explain the non-obvious formulas:
//!   * `probability` in the transition graph is row-normalised over *all*
//!     outgoing transitions of the source app, including the self transition, so
//!     every matrix row sums to ~1.0. Self edges are reported per node
//!     (`self_probability`) but never as edges.
//!   * `context_switch_cost_ms` charges half of the previous dwell time (capped
//!     at 60s) per switch: the re-orientation cost of rebuilding context after
//!     leaving an app is bounded by roughly half a minute of lost focus.
//!   * robust z-scores use median + 1.4826*MAD so a single huge day cannot drag
//!     the baseline, with a std-dev fallback when MAD collapses to 0.

use chrono::{Datelike, Duration as ChronoDuration, NaiveDate};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

// ---------------------------------------------------------------------------
// Shared input row
// ---------------------------------------------------------------------------

/// One `sessions` row reduced to the fields the analytics need.
#[derive(Debug, Clone)]
pub struct InsightSession {
    pub class: String,
    /// RFC3339 UTC start timestamp.
    pub started_at: String,
    /// RFC3339 UTC end timestamp; `None` for the still-running session.
    pub ended_at: Option<String>,
    pub duration_ms: i64,
    /// Local calendar date (`YYYY-MM-DD`).
    pub date_local: String,
}

impl InsightSession {
    /// Start instant in milliseconds since the Unix epoch, if parseable.
    fn start_ms(&self) -> Option<i64> {
        parse_rfc3339_ms(&self.started_at)
    }

    /// End instant; an open session is treated as ending when it started, which
    /// makes its gap 0 instead of a nonsense negative/huge value.
    fn end_ms(&self) -> Option<i64> {
        self.ended_at
            .as_deref()
            .and_then(parse_rfc3339_ms)
            .or_else(|| self.start_ms())
    }

    /// Positive dwell time used by dwell averages and the switch-cost model.
    fn dwell_ms(&self) -> i64 {
        self.duration_ms.max(0)
    }
}

fn parse_rfc3339_ms(ts: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|dt| dt.timestamp_millis())
}

/// Today's local calendar date as `YYYY-MM-DD`.
pub fn today_local() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// Default `from` for endpoints that take an inclusive range: 6 days ago, so a
/// bare request covers the last 7 local days.
pub fn default_from() -> String {
    (chrono::Local::now() - ChronoDuration::days(6))
        .format("%Y-%m-%d")
        .to_string()
}

fn date_shift(date: &str, days: i64) -> Option<String> {
    NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .ok()
        .map(|d| (d + ChronoDuration::days(days)).format("%Y-%m-%d").to_string())
}

fn parse_date(date: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()
}

/// Weekday index with Monday = 0 .. Sunday = 6.
fn weekday_mon0(date: NaiveDate) -> u8 {
    date.weekday().num_days_from_monday() as u8
}

// ---------------------------------------------------------------------------
// Small numeric helpers
// ---------------------------------------------------------------------------

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn median(values: &mut [f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = values.len();
    if n % 2 == 1 {
        values[n / 2]
    } else {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    }
}

/// Pearson correlation. Returns 0.0 for n < 3 or a zero-variance input so the
/// JSON never carries NaN/Inf.
fn pearson(xs: &[f64], ys: &[f64]) -> f64 {
    if xs.len() != ys.len() || xs.len() < 3 {
        return 0.0;
    }
    let mx = mean(xs);
    let my = mean(ys);
    let mut sxy = 0.0;
    let mut sxx = 0.0;
    let mut syy = 0.0;
    for (x, y) in xs.iter().zip(ys.iter()) {
        let dx = x - mx;
        let dy = y - my;
        sxy += dx * dy;
        sxx += dx * dx;
        syy += dy * dy;
    }
    let denom = (sxx * syy).sqrt();
    if denom <= f64::EPSILON || !denom.is_finite() {
        0.0
    } else {
        (sxy / denom).clamp(-1.0, 1.0)
    }
}

/// Ordinary least squares fit of `y = intercept + slope * x` where x is the
/// 0-based series index. Reports MAE and the residual standard deviation so the
/// forecast can draw a prediction interval.
struct Ols {
    slope: f64,
    intercept: f64,
    r2: f64,
    mae: f64,
    residual_std: f64,
}

fn ols(values: &[f64]) -> Ols {
    let n = values.len();
    if n == 0 {
        return Ols {
            slope: 0.0,
            intercept: 0.0,
            r2: 0.0,
            mae: 0.0,
            residual_std: 0.0,
        };
    }
    let nf = n as f64;
    let (sum_x, sum_y, sum_xy, sum_xx) =
        values
            .iter()
            .enumerate()
            .fold((0.0, 0.0, 0.0, 0.0), |(sx, sy, sxy, sxx), (i, y)| {
                let x = i as f64;
                (sx + x, sy + y, sxy + x * y, sxx + x * x)
            });
    let denom = nf * sum_xx - sum_x * sum_x;
    let (slope, intercept) = if n > 1 && denom.abs() > f64::EPSILON {
        let b = (nf * sum_xy - sum_x * sum_y) / denom;
        let a = (sum_y - b * sum_x) / nf;
        (b, a)
    } else {
        (0.0, sum_y / nf)
    };

    let my = sum_y / nf;
    let mut ss_res = 0.0;
    let mut ss_tot = 0.0;
    let mut abs_err = 0.0;
    for (i, y) in values.iter().enumerate() {
        let pred = intercept + slope * i as f64;
        let err = y - pred;
        ss_res += err * err;
        abs_err += err.abs();
        ss_tot += (y - my) * (y - my);
    }
    let r2 = if ss_tot > f64::EPSILON {
        (1.0 - ss_res / ss_tot).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let residual_std = if n > 1 {
        (ss_res / (nf - 1.0)).sqrt()
    } else {
        0.0
    };

    Ols {
        slope,
        intercept,
        r2,
        mae: abs_err / nf,
        residual_std: if residual_std.is_finite() {
            residual_std
        } else {
            0.0
        },
    }
}

/// Weekday multiplicative factors (`factor[i]` = weekday i, Monday = 0):
/// mean of that weekday's totals divided by the overall mean. Falls back to 1.0
/// for weekdays with no observations and when the overall mean is 0.
/// Multiplicative weekday factors (`factor[i]` = weekday i, Monday = 0) that
/// shape the regression line into a per-weekday projection.
///
/// Only *active* days (value > 0) contribute. The input series is deliberately
/// zero-filled so the OLS trend sees every calendar day, but a weekday whose
/// only observations are those filled zeros must NOT collapse to a factor of 0:
/// that would predict zero usage for that weekday forever, which is what made
/// sparse histories produce an all-zero forecast. A weekday with no active day
/// in the window falls back to 1.0 (neutral), and every factor is clamped to
/// `0.2..=5.0` so a single freak day cannot blow up the projection.
fn weekday_factors(dates: &[String], values: &[i64]) -> [f64; 7] {
    const MIN_FACTOR: f64 = 0.2;
    const MAX_FACTOR: f64 = 5.0;

    let mut sums = [0.0f64; 7];
    let mut counts = [0usize; 7];
    let mut active_total = 0.0f64;
    let mut active_days = 0usize;
    for (date, value) in dates.iter().zip(values.iter()) {
        if *value <= 0 {
            continue;
        }
        if let Some(d) = parse_date(date) {
            let wd = weekday_mon0(d) as usize;
            sums[wd] += *value as f64;
            counts[wd] += 1;
            active_total += *value as f64;
            active_days += 1;
        }
    }
    // The baseline is the mean of active days, not of the zero-filled calendar.
    let overall = if active_days > 0 {
        active_total / active_days as f64
    } else {
        0.0
    };
    let mut factors = [1.0f64; 7];
    if overall > f64::EPSILON {
        for i in 0..7 {
            if counts[i] > 0 {
                let f = (sums[i] / counts[i] as f64) / overall;
                factors[i] = if f.is_finite() {
                    f.clamp(MIN_FACTOR, MAX_FACTOR)
                } else {
                    1.0
                };
            }
        }
    }
    factors
}

/// Robust z-score inputs: median and MAD-scaled deviation of the trailing
/// window. `scale` is 0.0 when both MAD and the std-dev fallback are 0.
fn robust_z(value: f64, window: &[f64]) -> (f64, f64) {
    if window.is_empty() {
        return (0.0, 0.0);
    }
    let mut sorted = window.to_vec();
    let med = median(&mut sorted);
    let mut devs: Vec<f64> = window.iter().map(|v| (v - med).abs()).collect();
    let mad = median(&mut devs);
    let mut scale = mad * 1.4826;
    if scale <= f64::EPSILON {
        // MAD collapses when >50% of the window is identical; fall back to the
        // sample standard deviation around the median.
        let var = window.iter().map(|v| (v - med) * (v - med)).sum::<f64>()
            / (window.len().max(2) - 1) as f64;
        scale = var.sqrt();
    }
    let z = if scale > f64::EPSILON {
        (value - med) / scale
    } else {
        0.0
    };
    (med, if z.is_finite() { z } else { 0.0 })
}

// ---------------------------------------------------------------------------
// 1. Transitions
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct TransitionNode {
    pub class: String,
    pub total_ms: i64,
    pub session_count: i64,
    pub share: f64,
    pub out_total: i64,
    pub self_probability: f64,
    pub avg_dwell_ms: i64,
}

#[derive(Debug, Serialize)]
pub struct TransitionEdge {
    pub from: String,
    pub to: String,
    pub count: i64,
    pub probability: f64,
    pub from_out_total: i64,
    pub avg_gap_ms: i64,
}

#[derive(Debug, Serialize)]
pub struct TransitionMatrix {
    pub classes: Vec<String>,
    pub values: Vec<Vec<f64>>,
}

#[derive(Debug, Serialize)]
pub struct TransitionFlow {
    pub from: String,
    pub to: String,
    pub count: i64,
    pub probability: f64,
}

#[derive(Debug, Serialize)]
pub struct TransitionsResponse {
    #[serde(rename = "from")]
    pub from: String,
    #[serde(rename = "to")]
    pub to: String,
    pub min_ms: i64,
    pub limit: usize,
    pub total_transitions: i64,
    pub total_sessions: i64,
    pub nodes: Vec<TransitionNode>,
    pub edges: Vec<TransitionEdge>,
    pub matrix: TransitionMatrix,
    pub top_flows: Vec<TransitionFlow>,
}

/// Markov transition statistics over the chronologically ordered sessions.
///
/// Only the `limit` apps with the largest total_ms become nodes; edges between
/// non-kept apps are dropped, but `total_transitions` still counts every
/// consecutive pair of the filtered session list.
pub fn transitions(
    sessions: &[InsightSession],
    from: &str,
    to: &str,
    min_ms: i64,
    limit: usize,
) -> TransitionsResponse {
    let total_sessions = sessions.len() as i64;
    let all_total_ms: i64 = sessions.iter().map(|s| s.dwell_ms()).sum();

    // Node ranking by total_ms (ties broken by session count, then class name
    // for a stable, deterministic order).
    struct Agg {
        total_ms: i64,
        session_count: i64,
        dwell_sum: i64,
    }
    let mut agg: HashMap<&str, Agg> = HashMap::new();
    for s in sessions {
        let e = agg.entry(s.class.as_str()).or_insert(Agg {
            total_ms: 0,
            session_count: 0,
            dwell_sum: 0,
        });
        e.total_ms += s.dwell_ms();
        e.session_count += 1;
        e.dwell_sum += s.dwell_ms();
    }
    let mut ranked: Vec<(&str, i64, i64, i64)> = agg
        .iter()
        .map(|(c, a)| (*c, a.total_ms, a.session_count, a.dwell_sum))
        .collect();
    ranked.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then(b.2.cmp(&a.2))
            .then(a.0.cmp(b.0))
    });
    ranked.truncate(limit);
    let kept: HashSet<&str> = ranked.iter().map(|(c, ..)| *c).collect();
    let classes: Vec<String> = ranked.iter().map(|(c, ..)| c.to_string()).collect();
    let index_of: HashMap<&str, usize> = classes
        .iter()
        .enumerate()
        .map(|(i, c)| (c.as_str(), i))
        .collect();

    // Count every consecutive pair, then keep the edges between kept nodes.
    let mut out_total: HashMap<&str, i64> = HashMap::new();
    let mut self_count: HashMap<&str, i64> = HashMap::new();
    let mut edge_count: HashMap<(String, String), i64> = HashMap::new();
    let mut edge_gap_sum: HashMap<(String, String), i64> = HashMap::new();
    let mut total_transitions = 0i64;

    for pair in sessions.windows(2) {
        let prev = &pair[0];
        let next = &pair[1];
        total_transitions += 1;
        *out_total.entry(prev.class.as_str()).or_insert(0) += 1;
        if prev.class == next.class {
            *self_count.entry(prev.class.as_str()).or_insert(0) += 1;
        }
        if kept.contains(prev.class.as_str()) && kept.contains(next.class.as_str()) {
            let key = (prev.class.clone(), next.class.clone());
            *edge_count.entry(key.clone()).or_insert(0) += 1;
            // Gap is clamped to >= 0: clock jumps and overlapping sessions would
            // otherwise produce negative gaps.
            let gap = next
                .start_ms()
                .zip(prev.end_ms())
                .map(|(a, b)| (a - b).max(0))
                .unwrap_or(0);
            *edge_gap_sum.entry(key).or_insert(0) += gap;
        }
    }

    let mut nodes: Vec<TransitionNode> = ranked
        .iter()
        .map(|(class, total_ms, session_count, dwell_sum)| {
            let out = *out_total.get(*class).unwrap_or(&0);
            let selfs = *self_count.get(*class).unwrap_or(&0);
            TransitionNode {
                class: (*class).to_string(),
                total_ms: *total_ms,
                session_count: *session_count,
                share: if all_total_ms > 0 {
                    *total_ms as f64 / all_total_ms as f64
                } else {
                    0.0
                },
                out_total: out,
                self_probability: if out > 0 {
                    selfs as f64 / out as f64
                } else {
                    0.0
                },
                avg_dwell_ms: if *session_count > 0 {
                    dwell_sum / session_count
                } else {
                    0
                },
            }
        })
        .collect();
    nodes.sort_by(|a, b| {
        b.total_ms
            .cmp(&a.total_ms)
            .then(b.session_count.cmp(&a.session_count))
            .then(a.class.cmp(&b.class))
    });

    let mut edges: Vec<TransitionEdge> = edge_count
        .iter()
        .filter(|((a, b), _)| a != b)
        .map(|((a, b), count)| {
            let out = *out_total.get(a.as_str()).unwrap_or(&0);
            let gap_sum = *edge_gap_sum.get(&(a.clone(), b.clone())).unwrap_or(&0);
            TransitionEdge {
                from: a.clone(),
                to: b.clone(),
                count: *count,
                probability: if out > 0 {
                    *count as f64 / out as f64
                } else {
                    0.0
                },
                from_out_total: out,
                avg_gap_ms: if *count > 0 { gap_sum / *count } else { 0 },
            }
        })
        .collect();
    edges.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then(b.probability.partial_cmp(&a.probability).unwrap_or(std::cmp::Ordering::Equal))
            .then(a.from.cmp(&b.from))
            .then(a.to.cmp(&b.to))
    });

    // Row-normalised transition matrix over the kept classes.
    let mut values = vec![vec![0.0f64; classes.len()]; classes.len()];
    for ((a, b), count) in edge_count.iter() {
        if let (Some(&i), Some(&j)) = (index_of.get(a.as_str()), index_of.get(b.as_str())) {
            let out = *out_total.get(a.as_str()).unwrap_or(&0);
            if out > 0 {
                values[i][j] = *count as f64 / out as f64;
            }
        }
    }

    let mut top_flows: Vec<TransitionFlow> = edges
        .iter()
        .map(|e| TransitionFlow {
            from: e.from.clone(),
            to: e.to.clone(),
            count: e.count,
            probability: e.probability,
        })
        .collect();
    top_flows.sort_by(|a, b| {
        b.probability
            .partial_cmp(&a.probability)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.count.cmp(&a.count))
            .then(a.from.cmp(&b.from))
            .then(a.to.cmp(&b.to))
    });
    top_flows.truncate(20);

    TransitionsResponse {
        from: from.to_string(),
        to: to.to_string(),
        min_ms,
        limit,
        total_transitions,
        total_sessions,
        nodes,
        edges,
        matrix: TransitionMatrix { classes, values },
        top_flows,
    }
}

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

fn finite(v: f64) -> f64 {
    if v.is_finite() {
        v
    } else {
        0.0
    }
}

// ---------------------------------------------------------------------------
// 3. Rhythm
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct RhythmCell {
    pub weekday: u8,
    pub hour: u8,
    pub total_ms: i64,
    pub avg_ms: i64,
    pub days: i64,
}

#[derive(Debug, Serialize)]
pub struct RhythmResponse {
    #[serde(rename = "from")]
    pub from: String,
    #[serde(rename = "to")]
    pub to: String,
    pub max_avg_ms: i64,
    pub cells: Vec<RhythmCell>,
}

/// Average weekday x hour usage. `avg_ms` divides the cell total by the number
/// of distinct dates in the range that fall on that weekday, so a weekday that
/// only occurs twice is not diluted by the other five days.
pub fn rhythm(
    rows: &[(String, u8, i64)],
    from: &str,
    to: &str,
) -> RhythmResponse {
    let mut weekday_dates: [HashSet<String>; 7] = Default::default();
    if let (Some(a), Some(b)) = (parse_date(from), parse_date(to)) {
        let mut cur = a;
        while cur <= b {
            weekday_dates[weekday_mon0(cur) as usize].insert(cur.format("%Y-%m-%d").to_string());
            cur += ChronoDuration::days(1);
        }
    }

    let mut totals: HashMap<(u8, u8), i64> = HashMap::new();
    for (date, hour, total) in rows {
        let Some(d) = parse_date(date) else { continue };
        if *hour > 23 {
            continue;
        }
        *totals.entry((weekday_mon0(d), *hour)).or_insert(0) += total;
    }

    let mut cells: Vec<RhythmCell> = totals
        .into_iter()
        .filter(|(_, total)| *total > 0)
        .map(|((weekday, hour), total)| {
            let days = weekday_dates[weekday as usize].len() as i64;
            RhythmCell {
                weekday,
                hour,
                total_ms: total,
                avg_ms: if days > 0 { total / days } else { 0 },
                days,
            }
        })
        .collect();
    cells.sort_by(|a, b| a.weekday.cmp(&b.weekday).then(a.hour.cmp(&b.hour)));
    let max_avg_ms = cells.iter().map(|c| c.avg_ms).max().unwrap_or(0);

    RhythmResponse {
        from: from.to_string(),
        to: to.to_string(),
        max_avg_ms,
        cells,
    }
}

// ---------------------------------------------------------------------------
// 4. Fragmentation
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct FragmentationDay {
    pub date: String,
    pub session_count: i64,
    pub switch_count: i64,
    pub avg_dwell_ms: i64,
    pub median_dwell_ms: i64,
    pub short_session_ratio: f64,
    pub longest_focus_ms: i64,
    pub focus_blocks: i64,
    pub active_ms: i64,
    pub context_switch_cost_ms: i64,
}

#[derive(Debug, Serialize)]
pub struct WorstDay {
    pub date: String,
    pub switch_count: i64,
    pub avg_dwell_ms: i64,
}

#[derive(Debug, Serialize)]
pub struct FragmentationResponse {
    #[serde(rename = "from")]
    pub from: String,
    #[serde(rename = "to")]
    pub to: String,
    pub days: Vec<FragmentationDay>,
    pub avg_dwell_ms: i64,
    pub median_dwell_ms: i64,
    pub avg_switch_count: i64,
    pub short_session_ratio: f64,
    pub switches_per_hour: f64,
    pub trend_slope_ms_per_day: f64,
    pub worst_day: Option<WorstDay>,
}

/// Sessions shorter than this count as "short" (fragmented attention).
const SHORT_SESSION_MS: i64 = 60_000;
/// A focus block is a run of same-class sessions spanning at least 15 minutes.
const FOCUS_BLOCK_MS: i64 = 15 * 60_000;
/// Re-orientation cost per switch is capped at one minute of the previous dwell.
const SWITCH_COST_CAP_MS: i64 = 60_000;
/// Fraction of the pre-switch dwell charged as lost re-orientation time.
const SWITCH_COST_FACTOR: f64 = 0.5;

/// Cost of one switch: half of the previous dwell, capped at 60s.
///
/// Rationale: leaving an app and coming back costs the user time to rebuild
/// context. Charging half the previous dwell (bounded by a minute) keeps the
/// estimate conservative and independent of session length for long sessions.
pub fn switch_cost_ms(prev_dwell_ms: i64) -> i64 {
    let capped = prev_dwell_ms.max(0).min(SWITCH_COST_CAP_MS);
    (capped as f64 * SWITCH_COST_FACTOR).round() as i64
}

pub fn fragmentation(sessions: &[InsightSession], from: &str, to: &str) -> FragmentationResponse {
    // Group by local date, preserving the chronological order of the input.
    let mut by_day: Vec<(String, Vec<&InsightSession>)> = Vec::new();
    let mut day_index: HashMap<String, usize> = HashMap::new();
    for s in sessions {
        match day_index.get(&s.date_local) {
            Some(&i) => by_day[i].1.push(s),
            None => {
                day_index.insert(s.date_local.clone(), by_day.len());
                by_day.push((s.date_local.clone(), vec![s]));
            }
        }
    }
    by_day.sort_by(|a, b| a.0.cmp(&b.0));

    let mut days = Vec::with_capacity(by_day.len());
    let mut all_dwells: Vec<f64> = Vec::new();
    let mut total_switches = 0i64;
    let mut total_active = 0i64;
    let mut total_sessions = 0i64;
    let mut total_short = 0i64;

    for (date, day_sessions) in by_day.iter() {
        let session_count = day_sessions.len() as i64;
        let mut dwells: Vec<f64> = day_sessions.iter().map(|s| s.dwell_ms() as f64).collect();
        let active_ms: i64 = day_sessions.iter().map(|s| s.dwell_ms()).sum();

        let mut switch_count = 0i64;
        let mut cost = 0i64;
        for pair in day_sessions.windows(2) {
            if pair[0].class != pair[1].class {
                switch_count += 1;
                cost += switch_cost_ms(pair[0].dwell_ms());
            }
        }

        let short = day_sessions
            .iter()
            .filter(|s| s.dwell_ms() < SHORT_SESSION_MS)
            .count() as i64;

        // Focus blocks: consecutive same-class sessions whose combined duration
        // reaches 15 minutes. A single long session is its own block.
        let mut focus_blocks = 0i64;
        let mut run_class: Option<&str> = None;
        let mut run_ms = 0i64;
        for s in day_sessions.iter() {
            if run_class == Some(s.class.as_str()) {
                run_ms += s.dwell_ms();
            } else {
                if run_ms >= FOCUS_BLOCK_MS {
                    focus_blocks += 1;
                }
                run_class = Some(s.class.as_str());
                run_ms = s.dwell_ms();
            }
        }
        if run_ms >= FOCUS_BLOCK_MS {
            focus_blocks += 1;
        }

        let longest_focus_ms = {
            // Longest unbroken same-class run, reported as the run duration.
            let mut best = 0i64;
            let mut cur_class: Option<&str> = None;
            let mut cur = 0i64;
            for s in day_sessions.iter() {
                if cur_class == Some(s.class.as_str()) {
                    cur += s.dwell_ms();
                } else {
                    best = best.max(cur);
                    cur_class = Some(s.class.as_str());
                    cur = s.dwell_ms();
                }
            }
            best.max(cur)
        };

        let day_avg_dwell = if session_count > 0 {
            active_ms / session_count
        } else {
            0
        };
        days.push(FragmentationDay {
            date: date.clone(),
            session_count,
            switch_count,
            avg_dwell_ms: day_avg_dwell,
            median_dwell_ms: median(&mut dwells) as i64,
            short_session_ratio: if session_count > 0 {
                short as f64 / session_count as f64
            } else {
                0.0
            },
            longest_focus_ms,
            focus_blocks,
            active_ms,
            context_switch_cost_ms: cost,
        });

        all_dwells.extend(day_sessions.iter().map(|s| s.dwell_ms() as f64));
        total_switches += switch_count;
        total_active += active_ms;
        total_sessions += session_count;
        total_short += short;
    }

    let avg_dwell_ms = if total_sessions > 0 {
        total_active / total_sessions
    } else {
        0
    };
    let median_dwell_ms = median(&mut all_dwells) as i64;
    let avg_switch_count = if days.is_empty() {
        0
    } else {
        total_switches / days.len() as i64
    };
    let short_session_ratio = if total_sessions > 0 {
        total_short as f64 / total_sessions as f64
    } else {
        0.0
    };
    let active_hours = total_active as f64 / 3_600_000.0;
    let switches_per_hour = if active_hours > f64::EPSILON {
        total_switches as f64 / active_hours
    } else {
        0.0
    };

    let trend_slope_ms_per_day = if days.len() >= 2 {
        let y: Vec<f64> = days.iter().map(|d| d.avg_dwell_ms as f64).collect();
        finite(ols(&y).slope)
    } else {
        0.0
    };

    let worst_day = days
        .iter()
        .max_by(|a, b| {
            a.switch_count
                .cmp(&b.switch_count)
                .then(b.avg_dwell_ms.cmp(&a.avg_dwell_ms))
                .then(a.date.cmp(&b.date))
        })
        .map(|d| WorstDay {
            date: d.date.clone(),
            switch_count: d.switch_count,
            avg_dwell_ms: d.avg_dwell_ms,
        });

    FragmentationResponse {
        from: from.to_string(),
        to: to.to_string(),
        days,
        avg_dwell_ms,
        median_dwell_ms,
        avg_switch_count,
        short_session_ratio,
        switches_per_hour: finite(switches_per_hour),
        trend_slope_ms_per_day,
        worst_day,
    }
}

// ---------------------------------------------------------------------------
// 5. Co-occurrence
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct CooccurrencePair {
    pub a: String,
    pub b: String,
    pub co_occurrences: i64,
    pub support: f64,
    pub lift: f64,
    pub jaccard: f64,
    pub avg_gap_ms: i64,
}

#[derive(Debug, Serialize)]
pub struct CooccurrenceNode {
    pub class: String,
    pub degree: i64,
    pub total_ms: i64,
}

#[derive(Debug, Serialize)]
pub struct CooccurrenceResponse {
    #[serde(rename = "from")]
    pub from: String,
    #[serde(rename = "to")]
    pub to: String,
    pub window_minutes: i64,
    pub limit: usize,
    pub pairs: Vec<CooccurrencePair>,
    pub nodes: Vec<CooccurrenceNode>,
}

/// App pairs used within `window_minutes` of each other.
///
/// For every day the sessions are walked in start order and each ordered pair of
/// distinct classes whose starts fall inside one window counts once: once a pair
/// has been counted, further hits of the same pair inside the same window are
/// ignored, so a burst of five interleaved sessions is one co-occurrence rather
/// than ten. The pair is stored with a stable alphabetical ordering so (a,b) and
/// (b,a) merge.
pub fn cooccurrence(
    sessions: &[InsightSession],
    from: &str,
    to: &str,
    window_minutes: i64,
    limit: usize,
) -> CooccurrenceResponse {
    let window_ms = window_minutes.max(0) * 60_000;
    let mut pair_counts: HashMap<(String, String), i64> = HashMap::new();
    let mut pair_gap_sum: HashMap<(String, String), i64> = HashMap::new();
    let mut pair_days: HashMap<(String, String), HashSet<String>> = HashMap::new();
    let mut class_days: HashMap<String, HashSet<String>> = HashMap::new();
    let mut class_total_ms: HashMap<String, i64> = HashMap::new();

    let mut by_day: Vec<(String, Vec<&InsightSession>)> = Vec::new();
    let mut day_index: HashMap<String, usize> = HashMap::new();
    for s in sessions {
        class_total_ms
            .entry(s.class.clone())
            .and_modify(|v| *v += s.dwell_ms())
            .or_insert(s.dwell_ms());
        match day_index.get(&s.date_local) {
            Some(&i) => by_day[i].1.push(s),
            None => {
                day_index.insert(s.date_local.clone(), by_day.len());
                by_day.push((s.date_local.clone(), vec![s]));
            }
        }
    }

    for (date, day_sessions) in by_day.iter() {
        let mut day_classes: HashSet<String> = HashSet::new();
        for s in day_sessions.iter() {
            day_classes.insert(s.class.clone());
        }
        for c in day_classes.iter() {
            class_days
                .entry(c.clone())
                .or_default()
                .insert(date.clone());
        }

        // Last count time per pair, to dedupe repeated hits inside one window.
        let mut last_hit: HashMap<(String, String), i64> = HashMap::new();
        for i in 0..day_sessions.len() {
            let Some(start_i) = day_sessions[i].start_ms() else {
                continue;
            };
            for prev in day_sessions[..i].iter() {
                let Some(start_j) = prev.start_ms() else {
                    continue;
                };
                let gap = start_i - start_j;
                if gap > window_ms {
                    continue;
                }
                if prev.class == day_sessions[i].class {
                    continue;
                }
                let key = if prev.class <= day_sessions[i].class {
                    (prev.class.clone(), day_sessions[i].class.clone())
                } else {
                    (day_sessions[i].class.clone(), prev.class.clone())
                };
                if let Some(&last) = last_hit.get(&key) {
                    if start_i - last <= window_ms {
                        continue;
                    }
                }
                last_hit.insert(key.clone(), start_i);
                *pair_counts.entry(key.clone()).or_insert(0) += 1;
                *pair_gap_sum.entry(key.clone()).or_insert(0) += gap.max(0);
                pair_days.entry(key).or_default().insert(date.clone());
            }
        }
    }

    let total_days = {
        let mut all: HashSet<&String> = HashSet::new();
        for set in class_days.values() {
            for d in set.iter() {
                all.insert(d);
            }
        }
        all.len() as f64
    };

    let mut pairs: Vec<CooccurrencePair> = pair_counts
        .iter()
        .map(|(key, count)| {
            let (a, b) = key;
            let days_both = pair_days.get(key).map(|s| s.len()).unwrap_or(0) as f64;
            let days_a = class_days.get(a).map(|s| s.len()).unwrap_or(0) as f64;
            let days_b = class_days.get(b).map(|s| s.len()).unwrap_or(0) as f64;
            let support = if total_days > 0.0 {
                days_both / total_days
            } else {
                0.0
            };
            let jaccard = if days_a + days_b - days_both > 0.0 {
                days_both / (days_a + days_b - days_both)
            } else {
                0.0
            };
            // lift = P(a and b) / (P(a) * P(b)) over days.
            let lift = if total_days > 0.0 && days_a > 0.0 && days_b > 0.0 {
                (days_both / total_days) / ((days_a / total_days) * (days_b / total_days))
            } else {
                0.0
            };
            let gap_sum = pair_gap_sum.get(key).copied().unwrap_or(0);
            CooccurrencePair {
                a: a.clone(),
                b: b.clone(),
                co_occurrences: *count,
                support: finite(support),
                lift: finite(lift),
                jaccard: finite(jaccard),
                avg_gap_ms: if *count > 0 { gap_sum / *count } else { 0 },
            }
        })
        .collect();
    pairs.sort_by(|x, y| {
        y.lift
            .partial_cmp(&x.lift)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(y.co_occurrences.cmp(&x.co_occurrences))
            .then(x.a.cmp(&y.a))
            .then(x.b.cmp(&y.b))
    });
    pairs.truncate(limit);

    let mut degree: HashMap<&str, i64> = HashMap::new();
    for p in pairs.iter() {
        *degree.entry(p.a.as_str()).or_insert(0) += 1;
        *degree.entry(p.b.as_str()).or_insert(0) += 1;
    }
    let mut nodes: Vec<CooccurrenceNode> = degree
        .into_iter()
        .map(|(class, degree)| CooccurrenceNode {
            class: class.to_string(),
            degree,
            total_ms: class_total_ms.get(class).copied().unwrap_or(0),
        })
        .collect();
    nodes.sort_by(|a, b| {
        b.degree
            .cmp(&a.degree)
            .then(b.total_ms.cmp(&a.total_ms))
            .then(a.class.cmp(&b.class))
    });

    CooccurrenceResponse {
        from: from.to_string(),
        to: to.to_string(),
        window_minutes,
        limit,
        pairs,
        nodes,
    }
}

// ---------------------------------------------------------------------------
// 6. Disruption impact
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct DisruptionDay {
    pub date: String,
    pub disruptions: i64,
    pub notifications: i64,
    pub clipboard: i64,
    pub efficiency: f64,
    pub focus_ratio: f64,
    pub active_ms: i64,
    pub avg_dwell_ms: i64,
    pub session_count: i64,
}

#[derive(Debug, Serialize)]
pub struct DisruptionCorrelations {
    pub efficiency_vs_disruptions: f64,
    pub focus_ratio_vs_disruptions: f64,
    pub avg_dwell_vs_disruptions: f64,
    pub active_vs_disruptions: f64,
}

#[derive(Debug, Serialize)]
pub struct DisruptionBucket {
    pub count: usize,
    pub avg_efficiency: f64,
    pub avg_focus_ratio: f64,
}

#[derive(Debug, Serialize)]
pub struct DisruptionImpactResponse {
    #[serde(rename = "from")]
    pub from: String,
    #[serde(rename = "to")]
    pub to: String,
    pub n_days: usize,
    pub days: Vec<DisruptionDay>,
    pub correlations: DisruptionCorrelations,
    pub high_disruption_days: DisruptionBucket,
    pub low_disruption_days: DisruptionBucket,
    pub insight: String,
}

pub fn disruption_impact(
    mut days: Vec<DisruptionDay>,
    from: &str,
    to: &str,
) -> DisruptionImpactResponse {    days.sort_by(|a, b| a.date.cmp(&b.date));

    let disruptions: Vec<f64> = days.iter().map(|d| d.disruptions as f64).collect();
    let efficiency: Vec<f64> = days.iter().map(|d| d.efficiency).collect();
    let focus: Vec<f64> = days.iter().map(|d| d.focus_ratio).collect();
    let dwell: Vec<f64> = days.iter().map(|d| d.avg_dwell_ms as f64).collect();
    let active: Vec<f64> = days.iter().map(|d| d.active_ms as f64).collect();

    let correlations = DisruptionCorrelations {
        efficiency_vs_disruptions: pearson(&disruptions, &efficiency),
        focus_ratio_vs_disruptions: pearson(&disruptions, &focus),
        avg_dwell_vs_disruptions: pearson(&disruptions, &dwell),
        active_vs_disruptions: pearson(&disruptions, &active),
    };

    // Top / bottom quartile by disruption count, at least one day on each side.
    let n = days.len();
    let bucket_size = if n == 0 {
        0
    } else {
        ((n as f64 * 0.25).ceil() as usize).max(1).min(n)
    };
    let mut by_disruptions: Vec<&DisruptionDay> = days.iter().collect();
    by_disruptions.sort_by(|a, b| {
        b.disruptions
            .cmp(&a.disruptions)
            .then(a.date.cmp(&b.date))
    });
    let high_slice = &by_disruptions[..bucket_size.min(n)];
    let low_start = n.saturating_sub(bucket_size);
    let low_slice = &by_disruptions[low_start..];

    let bucket = |slice: &[&DisruptionDay]| -> DisruptionBucket {
        let eff: Vec<f64> = slice.iter().map(|d| d.efficiency).collect();
        let foc: Vec<f64> = slice.iter().map(|d| d.focus_ratio).collect();
        DisruptionBucket {
            count: slice.len(),
            avg_efficiency: finite(mean(&eff)),
            avg_focus_ratio: finite(mean(&foc)),
        }
    };
    let high = bucket(high_slice);
    let low = bucket(low_slice);

    let insight = build_insight(&high, &low, correlations.efficiency_vs_disruptions, n);

    DisruptionImpactResponse {
        from: from.to_string(),
        to: to.to_string(),
        n_days: n,
        days,
        correlations,
        high_disruption_days: high,
        low_disruption_days: low,
        insight,
    }
}

/// One factual sentence, no exclamation marks or emoji.
fn build_insight(
    high: &DisruptionBucket,
    low: &DisruptionBucket,
    corr: f64,
    n_days: usize,
) -> String {
    if n_days == 0 {
        return "No activity was recorded in this range.".to_string();
    }
    if high.count == 0 || low.count == 0 {
        return "Not enough days to compare interruption levels.".to_string();
    }
    let delta = low.avg_efficiency - high.avg_efficiency;
    if delta.abs() < 0.5 {
        return format!(
            "Days with the most interruptions score within 1 point of the quietest days ({} vs {}).",
            high.avg_efficiency.round() as i64,
            low.avg_efficiency.round() as i64
        );
    }
    if delta > 0.0 {
        let corr_note = if corr < -0.1 {
            " The correlation is negative."
        } else {
            ""
        };
        format!(
            "Days with the most interruptions score {} points lower on average ({} vs {}).{}",
            delta.round() as i64,
            high.avg_efficiency.round() as i64,
            low.avg_efficiency.round() as i64,
            corr_note
        )
    } else {
        format!(
            "Days with the most interruptions score {} points higher on average ({} vs {}).",
            (-delta).round() as i64,
            high.avg_efficiency.round() as i64,
            low.avg_efficiency.round() as i64
        )
    }
}

// ---------------------------------------------------------------------------
// 7. Anomalies
// ---------------------------------------------------------------------------

/// Efficiency sub-score (0..100) from pre-aggregated day totals.
///
/// Same factors as `Database::efficiency_score` (focus 40 / fragmentation 30 /
/// late night 15 / disruptions 15) so the disruption-impact endpoint can score a
/// whole range from summary rows without a query per day.
pub fn efficiency_subscore(
    active_ms: i64,
    focused_ms: i64,
    session_count: i64,
    late_night_ms: i64,
    disruptions: i64,
) -> f64 {
    if active_ms <= 0 {
        return 0.0;
    }
    let focus_ratio = (focused_ms as f64 / active_ms as f64).clamp(0.0, 1.0);
    let avg_session_secs = active_ms as f64 / session_count.max(1) as f64 / 1000.0;
    let late_night_pct = (late_night_ms as f64 / active_ms as f64) * 100.0;

    let focus_score = (focus_ratio * 40.0).clamp(0.0, 40.0);
    let frag_score = if avg_session_secs >= 15.0 * 60.0 && avg_session_secs <= 90.0 * 60.0 {
        30.0
    } else if avg_session_secs < 15.0 * 60.0 {
        (avg_session_secs / (15.0 * 60.0)) * 30.0
    } else {
        30.0 - ((avg_session_secs - 90.0 * 60.0) / (180.0 * 60.0)) * 10.0
    };
    let late_score = (15.0 * (1.0 - late_night_pct / 100.0)).clamp(0.0, 15.0);
    let disruption_score = (15.0 * (1.0 - disruptions as f64 / 20.0)).clamp(0.0, 15.0);

    let score = focus_score + frag_score.max(0.0) + late_score + disruption_score;
    finite(score).clamp(0.0, 100.0)
}

#[derive(Debug, Serialize)]
pub struct AnomalyDay {
    pub date: String,
    pub active_ms: i64,
    pub baseline_ms: i64,
    pub z_score: f64,
    pub severity: String,
    pub reasons: Vec<String>,
    pub late_night_share: f64,
    pub session_count: i64,
    pub avg_dwell_ms: i64,
    pub weekday: u8,
}

#[derive(Debug, Serialize)]
pub struct AnomaliesResponse {
    #[serde(rename = "from")]
    pub from: String,
    #[serde(rename = "to")]
    pub to: String,
    pub window: i64,
    pub threshold: f64,
    pub baseline_days: usize,
    pub days: Vec<AnomalyDay>,
}

/// Per-day aggregates fed into the anomaly detector.
pub struct AnomalyInput {
    pub date: String,
    pub active_ms: i64,
    pub session_count: i64,
    pub avg_dwell_ms: i64,
    /// Share of the day's ms spent in local hours 23 and 0..=5.
    pub late_night_share: f64,
    pub short_session_ratio: f64,
}

/// Flag days that deviate from the user's own trailing habit.
///
/// The baseline is the median of the previous `window` days (a robust centre
/// that one binge day cannot move) and the z-score is scaled by 1.4826*MAD.
/// Days inside the first `window` days of the range have no full baseline and
/// are skipped.
pub fn anomalies(
    inputs: &[AnomalyInput],
    from: &str,
    to: &str,
    window: i64,
    threshold: f64,
) -> AnomaliesResponse {
    let by_date: HashMap<&str, &AnomalyInput> =
        inputs.iter().map(|i| (i.date.as_str(), i)).collect();
    let mut days: Vec<AnomalyDay> = Vec::new();
    let mut baseline_days = 0usize;

    let start = match parse_date(from) {
        Some(d) => d,
        None => {
            return AnomaliesResponse {
                from: from.to_string(),
                to: to.to_string(),
                window,
                threshold,
                baseline_days: 0,
                days,
            }
        }
    };
    let end = match parse_date(to) {
        Some(d) => d,
        None => {
            return AnomaliesResponse {
                from: from.to_string(),
                to: to.to_string(),
                window,
                threshold,
                baseline_days: 0,
                days,
            }
        }
    };

    let mut cur = start;
    while cur <= end {
        let key = cur.format("%Y-%m-%d").to_string();
        let index = (cur - start).num_days();
        if index < window {
            cur += ChronoDuration::days(1);
            continue;
        }
        let window_start = cur - ChronoDuration::days(window);
        let mut baseline: Vec<f64> = Vec::with_capacity(window as usize);
        let mut w = window_start;
        while w < cur {
            let wk = w.format("%Y-%m-%d").to_string();
            let value = by_date.get(wk.as_str()).map(|i| i.active_ms).unwrap_or(0);
            baseline.push(value as f64);
            w += ChronoDuration::days(1);
        }
        if baseline.len() > baseline_days {
            baseline_days = baseline.len();
        }

        let today = by_date.get(key.as_str());
        let active_ms = today.map(|i| i.active_ms).unwrap_or(0);
        let (med, z) = robust_z(active_ms as f64, &baseline);

        let late_night = today.map(|i| i.late_night_share).unwrap_or(0.0);
        let short_ratio = today.map(|i| i.short_session_ratio).unwrap_or(0.0);
        let session_count = today.map(|i| i.session_count).unwrap_or(0);
        let avg_dwell = today.map(|i| i.avg_dwell_ms).unwrap_or(0);

        let z_fired = z.abs() >= threshold;
        let late_fired = late_night >= 0.35;
        let short_fired = short_ratio >= 0.75;
        let mut reasons = Vec::new();
        if z_fired {
            let dir = if z >= 0.0 { "above" } else { "below" };
            reasons.push(format!(
                "Active time {:.1}σ {} your {}-day baseline",
                z.abs(),
                dir,
                window
            ));
        }
        if late_fired {
            reasons.push(format!(
                "Late-night usage {:.0}% of active time",
                late_night * 100.0
            ));
        }
        if short_fired {
            reasons.push(format!(
                "Short sessions {:.0}% of sessions",
                short_ratio * 100.0
            ));
        }

        if reasons.is_empty() {
            cur += ChronoDuration::days(1);
            continue;
        }

        let triggers = reasons.len();
        let severity = if z.abs() >= threshold * 1.5 || triggers >= 2 {
            "high"
        } else if z_fired {
            "medium"
        } else {
            "low"
        };

        days.push(AnomalyDay {
            date: key,
            active_ms,
            baseline_ms: med.round().max(0.0) as i64,
            z_score: finite(z),
            severity: severity.to_string(),
            reasons,
            late_night_share: finite(late_night),
            session_count,
            avg_dwell_ms: avg_dwell,
            weekday: weekday_mon0(cur),
        });
        cur += ChronoDuration::days(1);
    }

    let severity_rank = |s: &str| match s {
        "high" => 0,
        "medium" => 1,
        _ => 2,
    };
    days.sort_by(|a, b| {
        severity_rank(&a.severity)
            .cmp(&severity_rank(&b.severity))
            .then(
                b.z_score
                    .abs()
                    .partial_cmp(&a.z_score.abs())
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then(a.date.cmp(&b.date))
    });
    days.truncate(30);

    AnomaliesResponse {
        from: from.to_string(),
        to: to.to_string(),
        window,
        threshold,
        baseline_days,
        days,
    }
}

/// Hours counted as "late night" for the anomaly detector: 23:00 and 00:00-05:59.
///
/// The SQL in `db.rs::insight_anomalies` filters the same hours; this is the
/// single Rust-side definition, kept test-only so it cannot drift silently.
#[cfg(test)]
pub fn is_late_night_hour(hour: u8) -> bool {
    hour == 23 || hour <= 5
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn session(class: &str, start: &str, end: &str, dur: i64, date: &str) -> InsightSession {
        InsightSession {
            class: class.to_string(),
            started_at: start.to_string(),
            ended_at: Some(end.to_string()),
            duration_ms: dur,
            date_local: date.to_string(),
        }
    }

    #[test]
    fn ols_recovers_known_line() {
        // y = 10 + 2x over 5 points: slope 2, intercept 10, perfect fit.
        let values: Vec<f64> = (0..5).map(|x| 10.0 + 2.0 * x as f64).collect();
        let fit = ols(&values);
        assert!((fit.slope - 2.0).abs() < 1e-9, "slope {}", fit.slope);
        assert!((fit.intercept - 10.0).abs() < 1e-9, "intercept {}", fit.intercept);
        assert!((fit.r2 - 1.0).abs() < 1e-9, "r2 {}", fit.r2);
        assert!(fit.mae < 1e-9, "mae {}", fit.mae);
        assert!(fit.residual_std < 1e-9, "std {}", fit.residual_std);
    }

    #[test]
    fn ols_r2_and_errors_on_noisy_series() {
        // A flat series has no variance: r2 must be 0 rather than NaN.
        let flat = vec![5.0, 5.0, 5.0, 5.0];
        let fit = ols(&flat);
        assert_eq!(fit.slope, 0.0);
        assert_eq!(fit.r2, 0.0);
        assert_eq!(fit.residual_std, 0.0);

        // Perfectly anti-correlated series → slope negative, r2 = 1.
        let down = vec![10.0, 8.0, 6.0, 4.0];
        let fit = ols(&down);
        assert!((fit.slope + 2.0).abs() < 1e-9);
        assert!((fit.r2 - 1.0).abs() < 1e-9);

        // One-point series: no slope information, intercept = the value.
        let fit = ols(&[7.0]);
        assert_eq!(fit.slope, 0.0);
        assert_eq!(fit.intercept, 7.0);

        // Empty input must not panic or produce NaN.
        let fit = ols(&[]);
        assert_eq!(fit.slope, 0.0);
        assert_eq!(fit.intercept, 0.0);
        assert!(fit.r2.is_finite() && fit.mae.is_finite() && fit.residual_std.is_finite());
    }

    #[test]
    fn weekday_factors_scale_by_weekday_mean() {
        // 2026-08-31 is a Monday. Give Mondays double the overall mean.
        let dates = vec![
            "2026-08-31".to_string(), // Mon
            "2026-09-01".to_string(), // Tue
            "2026-09-02".to_string(), // Wed
            "2026-09-07".to_string(), // Mon
            "2026-09-08".to_string(), // Tue
            "2026-09-09".to_string(), // Wed
        ];
        let values = vec![200, 100, 100, 200, 100, 100];
        let factors = weekday_factors(&dates, &values);
        // overall mean = 133.33; Monday mean = 200 → 1.5
        assert!((factors[0] - 1.5).abs() < 1e-9, "monday {}", factors[0]);
        assert!((factors[1] - 0.75).abs() < 1e-9, "tuesday {}", factors[1]);
        // Weekdays without data fall back to 1.0.
        assert_eq!(factors[5], 1.0);
        assert_eq!(factors[6], 1.0);
    }

    #[test]
    fn weekday_factors_zero_mean_falls_back_to_one() {
        let dates = vec!["2026-08-31".to_string(), "2026-09-01".to_string()];
        let factors = weekday_factors(&dates, &[0, 0]);
        assert!(factors.iter().all(|f| *f == 1.0));
        let factors = weekday_factors(&[], &[]);
        assert!(factors.iter().all(|f| *f == 1.0));
    }

    #[test]
    fn weekday_factors_ignore_zero_filled_days_and_never_collapse_to_zero() {
        // 14 continuous days ending 2026-09-09. Thursdays (2026-08-27 and
        // 2026-09-03) have no activity at all, so the zero-filled series holds
        // only zeros for weekday 3: its factor must be exactly 1.0, not 0.0.
        let start = NaiveDate::parse_from_str("2026-08-27", "%Y-%m-%d").unwrap();
        let mut dates = Vec::new();
        let mut values = Vec::new();
        for i in 0..14 {
            let d = start + ChronoDuration::days(i);
            let value: i64 = match weekday_mon0(d) {
                3 => 0,          // Thursday: no active day in the window
                0 => 10_000_000, // Monday: 2x the other active days
                _ => 5_000_000,
            };
            dates.push(d.format("%Y-%m-%d").to_string());
            values.push(value);
        }

        let factors = weekday_factors(&dates, &values);
        assert_eq!(factors[3], 1.0, "zero-only weekday must be neutral");
        assert!(
            factors.iter().all(|f| f.is_finite() && *f > 0.0),
            "all factors finite and positive: {factors:?}"
        );
        // An active weekday keeps its real factor: Monday mean 10M over an
        // active-day mean of (10M + 5*5M)/6 = 5.833M → 12/7.
        assert!(
            (factors[0] - 12.0 / 7.0).abs() < 1e-9,
            "monday factor {}",
            factors[0]
        );
        assert!(factors[0] > 1.0, "monday should be above baseline");
        assert!(factors[1] < 1.0, "tuesday should be below baseline");

        // A future point on the zero-only weekday must still predict > 0.
        let totals: HashMap<String, i64> =
            dates.iter().cloned().zip(values.iter().cloned()).collect();
        let r = forecast(&totals, "", "2026-09-09", 14, 7);
        assert_eq!(r.forecast[0].date, "2026-09-10"); // a Thursday
        assert!(
            r.forecast[0].predicted_ms > 0,
            "thursday forecast {:?}",
            r.forecast[0]
        );
        assert!(r.tomorrow_projected_ms > 0, "tomorrow {}", r.tomorrow_projected_ms);
        assert!(
            r.weekday_factors.iter().all(|f| f.is_finite() && *f > 0.0),
            "{:?}",
            r.weekday_factors
        );

        // Empty input still yields seven neutral factors and no NaN.
        let empty = weekday_factors(&[], &[]);
        assert_eq!(empty, [1.0; 7]);
    }

    #[test]
    fn weekday_factors_are_clamped_to_a_sane_range() {
        // One enormous active day must not push its weekday factor to 1e9.
        // With 7 active days a single huge Monday would otherwise score 7.0.
        let dates = vec![
            "2026-08-31".to_string(), // Mon, huge
            "2026-09-01".to_string(),
            "2026-09-02".to_string(),
            "2026-09-03".to_string(),
            "2026-09-04".to_string(),
            "2026-09-05".to_string(),
            "2026-09-06".to_string(),
        ];
        let values = vec![1_000_000_000i64, 1_000, 1_000, 1_000, 1_000, 1_000, 1_000];
        let factors = weekday_factors(&dates, &values);
        assert!((factors[0] - 5.0).abs() < 1e-9, "clamped high {}", factors[0]);
        assert!((factors[1] - 0.2).abs() < 1e-9, "clamped low {}", factors[1]);
    }

    #[test]
    fn late_night_hours_match_the_sql_predicate() {
        // Mirrors `hour >= 23 OR hour <= 5` in db.rs::insight_anomalies.
        for hour in 0..24u8 {
            assert_eq!(
                is_late_night_hour(hour),
                hour == 23 || hour <= 5,
                "hour {hour}"
            );
        }
    }

    #[test]
    fn pearson_sign_and_guards() {
        let up = vec![1.0, 2.0, 3.0, 4.0];
        let down = vec![4.0, 3.0, 2.0, 1.0];
        assert!(pearson(&up, &down) < -0.99);
        assert!(pearson(&up, &up) > 0.99);
        // Zero variance → 0.0, not NaN.
        assert_eq!(pearson(&[1.0, 1.0, 1.0], &up), 0.0);
        // Fewer than 3 points → 0.0.
        assert_eq!(pearson(&[1.0, 2.0], &[2.0, 1.0]), 0.0);
        // Mismatched lengths are guarded too.
        assert_eq!(pearson(&[1.0, 2.0, 3.0], &[1.0, 2.0]), 0.0);
    }

    #[test]
    fn robust_z_score_ignores_single_spike() {
        // Baseline 10 with one spike to 100: median 10, MAD small → the spike
        // still scores high, but a normal day stays near 0.
        let mut window: Vec<f64> = vec![10.0; 14];
        window[13] = 100.0;
        let (med, z_spike) = robust_z(100.0, &window);
        assert_eq!(med, 10.0);
        assert!(z_spike > 2.0, "spike z {}", z_spike);
        let (_, z_normal) = robust_z(10.0, &window);
        assert!(z_normal.abs() < 0.001, "normal z {}", z_normal);

        // All-identical window → scale 0 → z 0 rather than NaN/Inf.
        let (med, z) = robust_z(50.0, &[10.0; 7]);
        assert_eq!(med, 10.0);
        assert_eq!(z, 0.0);

        // Empty window is guarded.
        let (med, z) = robust_z(5.0, &[]);
        assert_eq!(med, 0.0);
        assert_eq!(z, 0.0);
    }

    #[test]
    fn transition_matrix_rows_are_normalised() {
        // kitty → firefox → kitty → kitty
        let sessions = vec![
            session("kitty", "2026-09-01T00:00:00Z", "2026-09-01T00:10:00Z", 600_000, "2026-09-01"),
            session("firefox", "2026-09-01T00:11:00Z", "2026-09-01T00:20:00Z", 540_000, "2026-09-01"),
            session("kitty", "2026-09-01T00:21:00Z", "2026-09-01T00:30:00Z", 540_000, "2026-09-01"),
            session("kitty", "2026-09-01T00:31:00Z", "2026-09-01T00:40:00Z", 540_000, "2026-09-01"),
        ];
        let r = transitions(&sessions, "2026-09-01", "2026-09-01", 0, 12);
        assert_eq!(r.total_transitions, 3);
        assert_eq!(r.total_sessions, 4);
        // kitty out = 3 (k→f, f→k, k→k is 2 from kitty: k→f and k→k); firefox out = 1
        let kitty = r.nodes.iter().find(|n| n.class == "kitty").unwrap();
        assert_eq!(kitty.out_total, 2);
        assert!((kitty.self_probability - 0.5).abs() < 1e-9);
        // Self edges are excluded from `edges`.
        assert!(r.edges.iter().all(|e| e.from != e.to));
        // Every matrix row sums to ~1 where the row has outgoing transitions.
        for (i, row) in r.matrix.values.iter().enumerate() {
            let sum: f64 = row.iter().sum();
            let has_out = r.nodes[i].out_total > 0;
            if has_out {
                assert!((sum - 1.0).abs() < 1e-9, "row {i} sum {sum}");
            } else {
                assert_eq!(sum, 0.0);
            }
        }
        // matrix[kitty][kitty] = 0.5 (self), matrix[kitty][firefox] = 0.5
        let ki = r.matrix.classes.iter().position(|c| c == "kitty").unwrap();
        let fi = r.matrix.classes.iter().position(|c| c == "firefox").unwrap();
        assert!((r.matrix.values[ki][ki] - 0.5).abs() < 1e-9);
        assert!((r.matrix.values[ki][fi] - 0.5).abs() < 1e-9);
    }

    #[test]
    fn transition_edge_gap_is_clamped_and_probability_row_normalised() {
        // firefox ends 00:10, kitty starts 00:05 → negative gap clamps to 0.
        let sessions = vec![
            session("firefox", "2026-09-01T00:00:00Z", "2026-09-01T00:10:00Z", 600_000, "2026-09-01"),
            session("kitty", "2026-09-01T00:05:00Z", "2026-09-01T00:20:00Z", 900_000, "2026-09-01"),
        ];
        let r = transitions(&sessions, "2026-09-01", "2026-09-01", 0, 12);
        assert_eq!(r.edges.len(), 1);
        assert_eq!(r.edges[0].avg_gap_ms, 0);
        assert!((r.edges[0].probability - 1.0).abs() < 1e-9);
        assert_eq!(r.edges[0].from_out_total, 1);
        // No sessions → valid empty response.
        let empty = transitions(&[], "2026-09-01", "2026-09-01", 0, 12);
        assert_eq!(empty.total_transitions, 0);
        assert!(empty.nodes.is_empty() && empty.edges.is_empty());
        assert!(empty.matrix.classes.is_empty() && empty.matrix.values.is_empty());
    }

    #[test]
    fn fragmentation_cost_helper_is_half_of_capped_dwell() {
        assert_eq!(switch_cost_ms(0), 0);
        assert_eq!(switch_cost_ms(-500), 0);
        assert_eq!(switch_cost_ms(10_000), 5_000);
        // 60s cap: 120s dwell costs the same as 60s dwell.
        assert_eq!(switch_cost_ms(60_000), 30_000);
        assert_eq!(switch_cost_ms(120_000), 30_000);
        assert_eq!(switch_cost_ms(5_400_000), 30_000);
    }

    #[test]
    fn fragmentation_counts_switches_focus_blocks_and_short_sessions() {
        let sessions = vec![
            session("kitty", "2026-09-01T00:00:00Z", "2026-09-01T00:20:00Z", 1_200_000, "2026-09-01"),
            session("kitty", "2026-09-01T00:20:00Z", "2026-09-01T00:25:00Z", 300_000, "2026-09-01"),
            session("firefox", "2026-09-01T00:25:00Z", "2026-09-01T00:25:10Z", 10_000, "2026-09-01"),
            session("kitty", "2026-09-01T00:25:10Z", "2026-09-01T00:30:00Z", 290_000, "2026-09-01"),
        ];
        let r = fragmentation(&sessions, "2026-09-01", "2026-09-01");
        assert_eq!(r.days.len(), 1);
        let d = &r.days[0];
        assert_eq!(d.session_count, 4);
        // k→k is not a switch; k→f and f→k are.
        assert_eq!(d.switch_count, 2);
        assert_eq!(d.short_session_ratio, 0.25);
        // First two kitty sessions = 25 min ≥ 15 min → one focus block.
        assert_eq!(d.focus_blocks, 1);
        assert_eq!(d.longest_focus_ms, 1_500_000);
        assert_eq!(d.active_ms, 1_800_000);
        // cost = min(20min,60s)*0.5 + min(10s,60s)*0.5 = 30_000 + 5_000
        assert_eq!(d.context_switch_cost_ms, 35_000);
        assert_eq!(r.worst_day.as_ref().unwrap().date, "2026-09-01");
        // Empty input stays valid.
        let empty = fragmentation(&[], "2026-09-01", "2026-09-01");
        assert_eq!(empty.avg_dwell_ms, 0);
        assert_eq!(empty.switches_per_hour, 0.0);
        assert!(empty.worst_day.is_none());
    }

    #[test]
    fn forecast_trend_and_intervals_are_finite() {
        let mut totals = HashMap::new();
        // 10 rising days ending 2026-09-10.
        for i in 0..10 {
            let date = format!("2026-09-{:02}", i + 1);
            totals.insert(date, (i as i64 + 1) * 1_000_000);
        }
        let r = forecast(&totals, "firefox", "2026-09-10", 10, 3);
        assert_eq!(r.points.len(), 10);
        assert_eq!(r.forecast.len(), 3);
        assert!(r.slope_ms_per_day > 0.0);
        assert_eq!(r.trend, "up");
        assert!(r.points.iter().all(|p| p.lower_ms <= p.upper_ms));
        assert!(r.forecast.iter().all(|p| p.lower_ms <= p.upper_ms));
        // Empty data must not panic and must stay finite.
        let r = forecast(&HashMap::new(), "", "2026-09-10", 30, 7);
        assert_eq!(r.points.len(), 30);
        assert_eq!(r.forecast.len(), 7);
        assert_eq!(r.avg_ms, 0.0);
        assert_eq!(r.trend, "flat");
        assert!(r.slope_ms_per_day.is_finite() && r.residual_std_ms.is_finite());
        assert!(r.points.iter().all(|p| p.lower_ms >= 0 && p.upper_ms >= 0));
    }

    #[test]
    fn cooccurrence_dedupes_within_window_and_computes_lift() {
        let sessions = vec![
            session("kitty", "2026-09-01T00:00:00Z", "2026-09-01T00:01:00Z", 60_000, "2026-09-01"),
            session("firefox", "2026-09-01T00:02:00Z", "2026-09-01T00:03:00Z", 60_000, "2026-09-01"),
            session("kitty", "2026-09-01T00:04:00Z", "2026-09-01T00:05:00Z", 60_000, "2026-09-01"),
            session("firefox", "2026-09-01T00:06:00Z", "2026-09-01T00:07:00Z", 60_000, "2026-09-01"),
        ];
        let r = cooccurrence(&sessions, "2026-09-01", "2026-09-01", 30, 10);
        assert_eq!(r.pairs.len(), 1);
        // All four starts are inside one 30-minute window → a single hit.
        assert_eq!(r.pairs[0].co_occurrences, 1);
        assert_eq!(r.pairs[0].a, "firefox");
        assert_eq!(r.pairs[0].b, "kitty");
        assert!((r.pairs[0].support - 1.0).abs() < 1e-9);
        assert!((r.pairs[0].jaccard - 1.0).abs() < 1e-9);
        assert!((r.pairs[0].lift - 1.0).abs() < 1e-9);
        assert_eq!(r.nodes.len(), 2);
        assert!(r.nodes.iter().all(|n| n.degree == 1));
        // Empty input stays valid.
        let empty = cooccurrence(&[], "2026-09-01", "2026-09-01", 30, 10);
        assert!(empty.pairs.is_empty() && empty.nodes.is_empty());
    }

    #[test]
    fn anomaly_detector_flags_spike_and_late_night() {
        // 14 quiet days then a spike day; window=14 skips the first 14 days.
        let mut inputs: Vec<AnomalyInput> = (0..14)
            .map(|i| AnomalyInput {
                date: format!("2026-08-{:02}", i + 1),
                active_ms: 8_000_000,
                session_count: 100,
                avg_dwell_ms: 60_000,
                late_night_share: 0.0,
                short_session_ratio: 0.1,
            })
            .collect();
        inputs.push(AnomalyInput {
            date: "2026-08-15".to_string(),
            active_ms: 16_000_000,
            session_count: 520,
            avg_dwell_ms: 9_000,
            late_night_share: 0.41,
            short_session_ratio: 0.8,
        });
        let r = anomalies(&inputs, "2026-08-01", "2026-08-15", 14, 2.0);
        assert_eq!(r.baseline_days, 14);
        assert_eq!(r.days.len(), 1);
        let d = &r.days[0];
        assert_eq!(d.date, "2026-08-15");
        assert_eq!(d.baseline_ms, 8_000_000);
        // Flat baseline → std fallback 0 → z 0, but late-night and short-session
        // triggers still fire, and two triggers make it high severity.
        assert_eq!(d.z_score, 0.0);
        assert_eq!(d.severity, "high");
        assert_eq!(d.reasons.len(), 2);
        // Days inside the first window are never reported.
        let r = anomalies(&inputs, "2026-08-01", "2026-08-10", 14, 2.0);
        assert!(r.days.is_empty());
        assert_eq!(r.baseline_days, 0);
    }

    #[test]
    fn rhythm_averages_over_matching_weekdays() {
        // 2026-08-31 (Mon) and 2026-09-07 (Mon) → 2 Mondays.
        let rows = vec![
            ("2026-08-31".to_string(), 9u8, 1_000_000i64),
            ("2026-09-07".to_string(), 9, 3_000_000),
            ("2026-09-01".to_string(), 9, 500_000), // Tuesday
        ];
        let r = rhythm(&rows, "2026-08-31", "2026-09-07");
        let monday = r.cells.iter().find(|c| c.weekday == 0 && c.hour == 9).unwrap();
        assert_eq!(monday.total_ms, 4_000_000);
        assert_eq!(monday.days, 2);
        assert_eq!(monday.avg_ms, 2_000_000);
        let tuesday = r.cells.iter().find(|c| c.weekday == 1).unwrap();
        assert_eq!(tuesday.days, 1);
        assert_eq!(tuesday.avg_ms, 500_000);
        assert_eq!(r.max_avg_ms, 2_000_000);
        // Empty rows → valid empty response.
        let empty = rhythm(&[], "2026-08-31", "2026-09-07");
        assert!(empty.cells.is_empty());
        assert_eq!(empty.max_avg_ms, 0);
    }

    #[test]
    fn disruption_impact_buckets_and_insight() {
        let day = |date: &str, disruptions: i64, efficiency: f64| DisruptionDay {
            date: date.to_string(),
            disruptions,
            notifications: disruptions,
            clipboard: 0,
            efficiency,
            focus_ratio: 0.4,
            active_ms: 8_000_000,
            avg_dwell_ms: 45_000,
            session_count: 300,
        };
        let days = vec![
            day("2026-09-01", 0, 90.0),
            day("2026-09-02", 1, 88.0),
            day("2026-09-03", 2, 86.0),
            day("2026-09-04", 10, 60.0),
        ];
        let r = disruption_impact(days, "2026-09-01", "2026-09-04");
        assert_eq!(r.n_days, 4);
        // ceil(4*0.25) = 1 day per bucket.
        assert_eq!(r.high_disruption_days.count, 1);
        assert_eq!(r.low_disruption_days.count, 1);
        assert_eq!(r.high_disruption_days.avg_efficiency, 60.0);
        assert_eq!(r.low_disruption_days.avg_efficiency, 90.0);
        assert!(r.correlations.efficiency_vs_disruptions < 0.0);
        assert!(r.insight.contains("30 points lower"), "{}", r.insight);
        assert!(!r.insight.contains('!'));
        // Empty input stays valid.
        let empty = disruption_impact(Vec::new(), "2026-09-01", "2026-09-04");
        assert_eq!(empty.n_days, 0);
        assert_eq!(empty.correlations.efficiency_vs_disruptions, 0.0);
        assert_eq!(empty.high_disruption_days.count, 0);
        assert!(!empty.insight.is_empty());
    }
}
