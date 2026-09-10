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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::insights::forecast::forecast;
    use chrono::{Duration as ChronoDuration, NaiveDate};
    use std::collections::HashMap;

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
}
