use serde::Serialize;
use std::collections::{HashMap, HashSet};

use super::session::InsightSession;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::insights::test_support::session;

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
}
