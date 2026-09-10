use serde::Serialize;
use std::collections::{HashMap, HashSet};

use super::forecast::finite;
use super::session::InsightSession;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::insights::test_support::session;

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
}
