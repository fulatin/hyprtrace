use super::time::{parse_date, weekday_mon0};

// ---------------------------------------------------------------------------
// Small numeric helpers
// ---------------------------------------------------------------------------

pub(crate) fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

pub(crate) fn median(values: &mut [f64]) -> f64 {
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
pub(crate) fn pearson(xs: &[f64], ys: &[f64]) -> f64 {
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
pub(crate) struct Ols {
    pub(crate) slope: f64,
    pub(crate) intercept: f64,
    pub(crate) r2: f64,
    pub(crate) mae: f64,
    pub(crate) residual_std: f64,
}

pub(crate) fn ols(values: &[f64]) -> Ols {
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
pub(crate) fn weekday_factors(dates: &[String], values: &[i64]) -> [f64; 7] {
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
pub(crate) fn robust_z(value: f64, window: &[f64]) -> (f64, f64) {
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
