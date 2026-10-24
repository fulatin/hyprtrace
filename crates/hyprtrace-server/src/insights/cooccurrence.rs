use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};

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
///
/// How the scan stays cheap (this endpoint used to be an order of magnitude
/// slower than every other insights one): the walk keeps a sliding window of the
/// sessions inside `[start_i - window_ms, start_i]` and, per row, only looks at
/// the earliest in-window session of every *class*. That is exactly the partner
/// the naive back-scan would count for that class — the dedupe check below only
/// depends on the pair key and on the candidate's start, so no later session of
/// the same class can produce a different count or a different gap — but the
/// per-row cost no longer grows with the whole day prefix, and `started_at` is
/// parsed once per session instead of once per comparison. The pair maps are
/// keyed by interned class id instead of cloned `(String, String)`.
///
/// Ordering: the walk is defined on start instants, so each day is sorted by its
/// parsed start millisecond first (stable, so equal starts keep their
/// `ORDER BY started_at` order). For input already in start order — what
/// `db::insight_sessions` returns — the sort is a no-op and the output is
/// byte-for-byte what the naive scan produces. It also makes the result
/// independent of the timestamp text format (`Z` and `+00:00` sort differently
/// as strings but denote the same instant).
pub fn cooccurrence(
    sessions: &[InsightSession],
    from: &str,
    to: &str,
    window_minutes: i64,
    limit: usize,
) -> CooccurrenceResponse {
    let window_ms = window_minutes.max(0) * 60_000;

    // Intern class names and days once; everything below is keyed by a small
    // integer id, so the per-day scan never allocates a string.
    let mut class_ids: HashMap<&str, u32> = HashMap::new();
    let mut class_names: Vec<&str> = Vec::new();
    let mut class_total_ms: Vec<i64> = Vec::new();
    let mut class_days: Vec<HashSet<u32>> = Vec::new();

    let mut day_ids: HashMap<&str, u32> = HashMap::new();
    let mut all_days: HashSet<u32> = HashSet::new();
    // Per day: `(start_ms, class_id)` for the sessions with a parseable start.
    let mut by_day: Vec<Vec<(i64, u32)>> = Vec::new();

    for s in sessions {
        let class = *class_ids.entry(s.class.as_str()).or_insert_with(|| {
            class_names.push(s.class.as_str());
            class_total_ms.push(0);
            class_days.push(HashSet::new());
            (class_names.len() - 1) as u32
        });
        class_total_ms[class as usize] += s.dwell_ms();

        let day = *day_ids.entry(s.date_local.as_str()).or_insert_with(|| {
            by_day.push(Vec::new());
            (by_day.len() - 1) as u32
        });
        all_days.insert(day);
        class_days[class as usize].insert(day);

        // A session without a parseable start can be neither the outer nor the
        // inner session of a pair (both loops used to skip it), but it still
        // counts towards the class total and the per-class day set.
        if let Some(start) = s.start_ms() {
            by_day[day as usize].push((start, class));
        }
    }

    // Alphabetical rank per class id. The pair key has to be normalised by
    // *name* (`(firefox, kitty)`, never `(kitty, firefox)`), and ids are handed
    // out in first-seen order, so the ranks let the hot loop compare integers
    // while still producing exactly the names-ordered key.
    let mut class_rank: Vec<u32> = vec![0; class_names.len()];
    {
        let mut by_name: Vec<u32> = (0..class_names.len() as u32).collect();
        by_name.sort_by_key(|&id| class_names[id as usize]);
        for (rank, &id) in by_name.iter().enumerate() {
            class_rank[id as usize] = rank as u32;
        }
    }

    let mut pair_counts: HashMap<(u32, u32), i64> = HashMap::new();
    let mut pair_gap_sum: HashMap<(u32, u32), i64> = HashMap::new();
    let mut pair_days: HashMap<(u32, u32), HashSet<u32>> = HashMap::new();

    // Per-day scratch state; `window_by_class[c]` holds, in ascending order, the
    // indices of the in-window sessions of class `c`, and `classes_in_window` is
    // exactly the set of classes with a non-empty deque.
    let mut window_by_class: Vec<VecDeque<u32>> = vec![VecDeque::new(); class_names.len()];
    let mut classes_in_window: Vec<u32> = Vec::new();
    let mut window_order: VecDeque<u32> = VecDeque::new();
    let mut last_hit: HashMap<(u32, u32), i64> = HashMap::new();

    for (day, entries) in by_day.iter_mut().enumerate() {
        let day = day as u32;
        entries.sort_by_key(|&(start, _)| start);

        window_by_class.iter_mut().for_each(VecDeque::clear);
        classes_in_window.clear();
        window_order.clear();
        last_hit.clear();

        for i in 0..entries.len() {
            let (start_i, class_i) = entries[i];
            let rank_i = class_rank[class_i as usize];

            // Retire everything that fell out of the window. `window_order` and
            // the per-class deques list the same indices in the same order, so
            // evicting the global front evicts the class deque's front too, and
            // what is left at the front of a class deque is that class's earliest
            // in-window session.
            let cutoff = start_i - window_ms;
            while let Some(&front) = window_order.front() {
                if entries[front as usize].0 >= cutoff {
                    break;
                }
                window_order.pop_front();
                let class_front = entries[front as usize].1;
                let queued = &mut window_by_class[class_front as usize];
                debug_assert_eq!(queued.front().copied(), Some(front));
                queued.pop_front();
                if queued.is_empty() {
                    classes_in_window.retain(|&c| c != class_front);
                }
            }

            for &class_j in classes_in_window.iter() {
                if class_j == class_i {
                    continue; // a self pair is not a co-occurrence
                }
                let key = if class_rank[class_j as usize] <= rank_i {
                    (class_j, class_i)
                } else {
                    (class_i, class_j)
                };
                if let Some(&last) = last_hit.get(&key) {
                    if start_i - last <= window_ms {
                        continue; // this pair was already counted in this window
                    }
                }
                let earliest = window_by_class[class_j as usize]
                    .front()
                    .copied()
                    .expect("class listed in the window has a session") as usize;
                last_hit.insert(key, start_i);
                *pair_counts.entry(key).or_insert(0) += 1;
                *pair_gap_sum.entry(key).or_insert(0) += (start_i - entries[earliest].0).max(0);
                pair_days.entry(key).or_default().insert(day);
            }

            window_order.push_back(i as u32);
            let queued = &mut window_by_class[class_i as usize];
            if queued.is_empty() {
                classes_in_window.push(class_i);
            }
            queued.push_back(i as u32);
        }
    }

    let total_days = all_days.len() as f64;

    let mut pairs: Vec<CooccurrencePair> = pair_counts
        .iter()
        .map(|(&key, &count)| {
            let (a, b) = key;
            let days_both = pair_days.get(&key).map(|s| s.len()).unwrap_or(0) as f64;
            let days_a = class_days[a as usize].len() as f64;
            let days_b = class_days[b as usize].len() as f64;
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
            let gap_sum = pair_gap_sum.get(&key).copied().unwrap_or(0);
            CooccurrencePair {
                a: class_names[a as usize].to_string(),
                b: class_names[b as usize].to_string(),
                co_occurrences: count,
                support: finite(support),
                lift: finite(lift),
                jaccard: finite(jaccard),
                avg_gap_ms: if count > 0 { gap_sum / count } else { 0 },
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
            total_ms: class_ids
                .get(class)
                .map(|&id| class_total_ms[id as usize])
                .unwrap_or(0),
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

    // -----------------------------------------------------------------------
    // Equivalence oracle for the sliding-window rewrite.
    // -----------------------------------------------------------------------

    /// The pre-optimisation implementation, kept verbatim: it walked the whole
    /// prefix of the day for every session and re-parsed every `started_at` on
    /// every comparison (that is what made the endpoint O(n²) with an expensive
    /// constant). It lives on as the oracle the sliding window must match.
    fn reference_cooccurrence(
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
                class_days.entry(c.clone()).or_default().insert(date.clone());
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

    /// Whole-response comparison: every field, including the f64 metrics, must
    /// come out of the sliding window exactly as it left the naive scan.
    fn assert_same(label: &str, got: &CooccurrenceResponse, want: &CooccurrenceResponse) {
        assert_eq!(
            serde_json::to_value(got).expect("response serialises"),
            serde_json::to_value(want).expect("response serialises"),
            "{label}"
        );
    }

    /// Deterministic LCG so the randomised equivalence cases reproduce without
    /// adding a dev-dependency.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            self.0 >> 33
        }

        fn below(&mut self, n: u64) -> u64 {
            if n == 0 {
                return 0; // a zero-width distribution (used by the tie fixture)
            }
            self.next() % n
        }
    }

    /// 2026-09-01T00:00:00Z in milliseconds; the fixtures only need a fixed epoch
    /// so session counts and window overlap are reproducible.
    const FIXTURE_EPOCH_MS: i64 = 1_788_220_800_000;

    fn ts(ms: i64) -> String {
        chrono::DateTime::from_timestamp_millis(ms)
            .expect("fixture timestamps are valid")
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, false)
    }

    /// One local date worth of sessions in start order: `count` sessions over
    /// `classes`, each up to `max_dwell_ms` long and followed by up to
    /// `max_gap_ms` of idle time. The shape mirrors the live database (hundreds
    /// of short sessions a day over a dozen classes).
    fn day_of_sessions(
        rng: &mut Rng,
        day: usize,
        count: usize,
        classes: &[&str],
        max_dwell_ms: i64,
        max_gap_ms: i64,
    ) -> Vec<InsightSession> {
        let date = format!("2026-09-{:02}", day + 1);
        let mut out = Vec::with_capacity(count);
        let mut cursor = FIXTURE_EPOCH_MS + day as i64 * 86_400_000;
        for _ in 0..count {
            let class = classes[rng.below(classes.len() as u64) as usize];
            let dwell = rng.below(max_dwell_ms as u64) as i64;
            let start = cursor;
            out.push(session(class, &ts(start), &ts(start + dwell), dwell, &date));
            cursor = start + dwell + rng.below(max_gap_ms as u64) as i64;
        }
        out
    }

    const FIXTURE_CLASSES: [&str; 12] = [
        "firefox",
        "kitty",
        "dev.zed.zed",
        "osu!",
        "eclipse",
        "arknights.exe",
        "libreoffice-writer",
        "waydroid",
        "spotify",
        "code",
        "thunderbird",
        "mpv",
    ];

    /// Run `cooccurrence` and the oracle side by side over the whole window/limit
    /// grid and require byte-identical responses (f64 metrics included).
    ///
    /// The oracle is order-sensitive by construction (it walks the rows as
    /// given), so this only makes sense for start-ordered input; the order
    /// independence of the new walk is covered separately below.
    fn assert_matches_oracle(label: &str, sessions: &[InsightSession]) {
        for window in [-3_i64, 0, 1, 5, 30, 120, 1_440] {
            for limit in [1_usize, 8, 100] {
                let got = cooccurrence(sessions, "2026-09-01", "2026-09-07", window, limit);
                let want =
                    reference_cooccurrence(sessions, "2026-09-01", "2026-09-07", window, limit);
                assert_same(&format!("{label} window={window} limit={limit}"), &got, &want);
            }
        }
    }

    /// One fixture per case the rewrite could plausibly break: three randomised
    /// dense days, a single-session day, a same-class-only day, a pair exactly on
    /// the window edge and one millisecond past it, mixed `Z`/`+00:00`
    /// spellings, rows with an unparseable or empty `started_at`, and a
    /// duplicated start instant. Ordered by start so the oracle applies.
    fn realistic_fixture() -> Vec<InsightSession> {
        let mut rng = Rng(0x5eed);
        let mut sessions: Vec<InsightSession> = Vec::new();
        for day in 0..3 {
            sessions.extend(day_of_sessions(
                &mut rng,
                day,
                100,
                &FIXTURE_CLASSES,
                60_000,
                60_000,
            ));
        }
        // A day with exactly one session.
        sessions.push(session(
            "lone",
            "2026-09-04T00:00:00Z",
            "2026-09-04T00:01:00Z",
            60_000,
            "2026-09-04",
        ));
        // One class only: no pair can come out of this day.
        for i in 0..20 {
            sessions.push(session(
                "solo",
                &ts(FIXTURE_EPOCH_MS + 4 * 86_400_000 + i * 60_000),
                &ts(FIXTURE_EPOCH_MS + 4 * 86_400_000 + i * 60_000 + 30_000),
                30_000,
                "2026-09-05",
            ));
        }
        // `edge-b` is exactly one window after `edge-a`; `edge-c` is one
        // millisecond past a window after `edge-b`. Spellings are mixed on
        // purpose (`Z` and `+00:00` sort differently as text).
        sessions.push(session(
            "edge-a",
            "2026-09-06T00:00:00Z",
            "2026-09-06T00:01:00Z",
            60_000,
            "2026-09-06",
        ));
        sessions.push(session(
            "edge-b",
            "2026-09-06T00:30:00.000+00:00",
            "2026-09-06T00:31:00Z",
            60_000,
            "2026-09-06",
        ));
        sessions.push(session(
            "edge-c",
            "2026-09-06T01:00:00.001Z",
            "2026-09-06T01:01:00+00:00",
            60_000,
            "2026-09-06",
        ));
        // Rows the walk cannot place in time. They must stay invisible to the
        // pairs and still count for the per-class day sets and totals.
        sessions.push(session(
            "ghost",
            "not-a-timestamp",
            "2026-09-07T00:01:00Z",
            1_000,
            "2026-09-07",
        ));
        sessions.push(session("ghost", "", "", 1_000, "2026-09-07"));
        // A duplicated start instant (same instant, different class).
        let mut duplicate = sessions[10].clone();
        duplicate.class = "kitty".to_string();
        sessions.insert(11, duplicate);
        sessions
    }

    /// The sliding window must produce exactly what the naive prefix scan
    /// produced on start-ordered input, across windows and limits.
    #[test]
    fn cooccurrence_matches_the_naive_scan_on_start_ordered_input() {
        assert_matches_oracle("realistic", &realistic_fixture());

        // A burst where every session starts on the same instant: ties keep their
        // row order and the zero-gap window still counts each pair once.
        let mut rng = Rng(0xB007);
        let burst = day_of_sessions(&mut rng, 0, 60, &FIXTURE_CLASSES, 0, 0);
        assert_matches_oracle("burst", &burst);

        // Empty input stays valid and equals the oracle.
        assert_matches_oracle("empty", &[]);
    }

    /// The pure function does not require SQL-sorted rows: shuffled input (the
    /// oracle is order-sensitive, so it cannot be compared directly) must give
    /// the same answer as the start-ordered input, and that answer is what the
    /// oracle produces for the ordered form.
    #[test]
    fn cooccurrence_is_order_independent_on_shuffled_input() {
        let ordered = realistic_fixture();
        let mut rng = Rng(0x51F7);
        let mut shuffled = ordered.clone();
        // Fisher-Yates with the fixture RNG: a fixed seed, so a failure replays.
        for i in (1..shuffled.len()).rev() {
            let j = rng.below(i as u64 + 1) as usize;
            shuffled.swap(i, j);
        }
        assert_ne!(
            shuffled.iter().map(|s| &s.started_at).collect::<Vec<_>>(),
            ordered.iter().map(|s| &s.started_at).collect::<Vec<_>>(),
            "the shuffle must actually change the row order"
        );

        for window in [-3_i64, 0, 1, 30, 1_440] {
            let from_ordered = cooccurrence(&ordered, "2026-09-01", "2026-09-07", window, 10);
            let from_shuffled = cooccurrence(&shuffled, "2026-09-01", "2026-09-07", window, 10);
            let oracle =
                reference_cooccurrence(&ordered, "2026-09-01", "2026-09-07", window, 10);
            assert_same(&format!("shuffled window={window}"), &from_shuffled, &from_ordered);
            assert_same(&format!("shuffled-vs-oracle window={window}"), &from_shuffled, &oracle);
        }
    }

    /// The window edge is inclusive on the near side: `gap == window_ms` counts
    /// and `gap == window_ms + 1` does not. The sliding window evicts by
    /// `start_j < start_i - window_ms`, so this is the boundary that would drift
    /// first if that comparison were off by one.
    #[test]
    fn cooccurrence_window_boundary_counts_exactly_the_window() {
        let rows = vec![
            session(
                "a",
                "2026-09-01T00:00:00.000Z",
                "2026-09-01T00:01:00Z",
                60_000,
                "2026-09-01",
            ),
            session(
                "b",
                "2026-09-01T00:30:00.000Z",
                "2026-09-01T00:31:00Z",
                60_000,
                "2026-09-01",
            ),
            session(
                "c",
                "2026-09-01T01:00:00.001Z",
                "2026-09-01T01:01:00Z",
                60_000,
                "2026-09-01",
            ),
        ];
        let r = cooccurrence(&rows, "2026-09-01", "2026-09-01", 30, 10);
        assert_same(
            "boundary",
            &r,
            &reference_cooccurrence(&rows, "2026-09-01", "2026-09-01", 30, 10),
        );
        assert_eq!(r.pairs.len(), 1, "only the exact-window pair may count");
        assert_eq!(r.pairs[0].a, "a");
        assert_eq!(r.pairs[0].b, "b");
        assert_eq!(r.pairs[0].co_occurrences, 1);
        assert_eq!(r.pairs[0].avg_gap_ms, 1_800_000, "gap == window counts");
    }

    /// The walk is defined on start instants, not on row order. Rows handed over
    /// back to front (with `Z`/`+00:00` spellings that sort differently as text)
    /// must still produce the start-ordered answer: a long session, a burst five
    /// minutes into it, then a second pair two windows later.
    #[test]
    fn cooccurrence_walks_start_instants_not_row_order() {
        let rows = vec![
            session(
                "a",
                "2026-09-01T00:00:00Z",
                "2026-09-01T00:40:00Z",
                2_400_000,
                "2026-09-01",
            ),
            session(
                "b",
                "2026-09-01T00:05:00+00:00",
                "2026-09-01T00:06:00+00:00",
                60_000,
                "2026-09-01",
            ),
            session(
                "c",
                "2026-09-01T00:50:00Z",
                "2026-09-01T00:51:00Z",
                60_000,
                "2026-09-01",
            ),
            session(
                "a",
                "2026-09-01T01:20:00Z",
                "2026-09-01T01:21:00Z",
                60_000,
                "2026-09-01",
            ),
            session(
                "b",
                "2026-09-01T01:25:00Z",
                "2026-09-01T01:26:00Z",
                60_000,
                "2026-09-01",
            ),
        ];
        let reversed: Vec<InsightSession> = rows.iter().rev().cloned().collect();

        let want = cooccurrence(&rows, "2026-09-01", "2026-09-01", 45, 10);
        assert_same(
            "reversed row order",
            &cooccurrence(&reversed, "2026-09-01", "2026-09-01", 45, 10),
            &want,
        );

        // (a,b) is 5 min apart in the first window and again in the second;
        // (b,c) and (a,c) sit in the middle window and only count once there.
        assert_eq!(want.pairs.len(), 3);
        let ab = want
            .pairs
            .iter()
            .find(|p| p.a == "a" && p.b == "b")
            .expect("(a,b) pair");
        assert_eq!(ab.co_occurrences, 2);
        assert_eq!(ab.avg_gap_ms, 300_000);
        let bc = want
            .pairs
            .iter()
            .find(|p| p.a == "b" && p.b == "c")
            .expect("(b,c) pair");
        assert_eq!(bc.co_occurrences, 1);
        assert_eq!(bc.avg_gap_ms, 2_700_000);
    }

    /// The regression gate for this endpoint: the day scan must not fall back to
    /// walking the whole prefix. The old implementation is kept in-process as the
    /// control, so this asserts a *ratio* between two runs on the same machine
    /// (load-insensitive) rather than an absolute wall-clock budget; on the old
    /// code both sides are the same algorithm and the ratio collapses to ~1.
    #[test]
    fn cooccurrence_does_not_rescan_the_whole_day() {
        let mut rng = Rng(0xC0FFEE);
        let mut sessions: Vec<InsightSession> = Vec::new();
        for day in 0..4 {
            sessions.extend(day_of_sessions(
                &mut rng,
                day,
                900,
                &FIXTURE_CLASSES,
                60_000,
                60_000,
            ));
        }

        let started = std::time::Instant::now();
        let fast = cooccurrence(&sessions, "2026-09-01", "2026-09-04", 30, 10);
        let fast_elapsed = started.elapsed();

        let started = std::time::Instant::now();
        let slow = reference_cooccurrence(&sessions, "2026-09-01", "2026-09-04", 30, 10);
        let slow_elapsed = started.elapsed();

        assert_same("dense fixture", &fast, &slow);
        assert!(
            slow_elapsed > fast_elapsed * 5,
            "the co-occurrence scan is quadratic again: fixed={fast_elapsed:?} \
             naive={slow_elapsed:?} over {} sessions",
            sessions.len()
        );
    }

    /// Micro-benchmark used for the perf report (not part of the default suite):
    /// `cargo test -p hyprtrace-server --release -- --ignored --nocapture cooccurrence_bench`
    #[test]
    #[ignore]
    fn cooccurrence_bench_realistic_month() {
        let mut rng = Rng(0xBEEF);
        let mut sessions: Vec<InsightSession> = Vec::new();
        for day in 0..30 {
            sessions.extend(day_of_sessions(
                &mut rng,
                day,
                820,
                &FIXTURE_CLASSES,
                60_000,
                60_000,
            ));
        }

        let started = std::time::Instant::now();
        let out = cooccurrence(&sessions, "2026-09-01", "2026-09-30", 30, 8);
        println!(
            "sliding window: {} sessions in {:?} ({} pairs)",
            sessions.len(),
            started.elapsed(),
            out.pairs.len()
        );

        // The naive scan only fits one day in a sane time; the month costs about
        // 30x one day (its per-day work is quadratic in that day's session count).
        let one_day: Vec<InsightSession> = sessions
            .iter()
            .filter(|s| s.date_local == "2026-09-01")
            .cloned()
            .collect();
        let started = std::time::Instant::now();
        let old = reference_cooccurrence(&one_day, "2026-09-01", "2026-09-01", 30, 8);
        println!(
            "naive scan: {} sessions in {:?} -> x30 days ~= {:?}",
            one_day.len(),
            started.elapsed(),
            started.elapsed() * 30
        );
        assert!(!old.pairs.is_empty());

        // What the ordering guarantee costs: one stable sort per day over the
        // same `(start_ms, class)` shape the walk builds. The SQL hands the rows
        // over already ordered, so that is the cheap case; a fully reversed day
        // is the worst case.
        let day_shape = || -> Vec<Vec<(i64, u32)>> {
            (0..30)
                .map(|d| {
                    (0..820_u32)
                        .map(|i| {
                            (
                                FIXTURE_EPOCH_MS + d as i64 * 86_400_000 + i as i64 * 60_000,
                                i % 12,
                            )
                        })
                        .collect()
                })
                .collect()
        };
        let mut ordered_days = day_shape();
        let started = std::time::Instant::now();
        for day in ordered_days.iter_mut() {
            day.sort_by_key(|&(start, _)| start);
        }
        println!(
            "per-day sort, 30x820 already ordered (the SQL case): {:?}",
            started.elapsed()
        );
        let mut reversed_days = day_shape();
        for day in reversed_days.iter_mut() {
            day.reverse();
        }
        let started = std::time::Instant::now();
        for day in reversed_days.iter_mut() {
            day.sort_by_key(|&(start, _)| start);
        }
        println!(
            "per-day sort, 30x820 fully reversed (worst case): {:?}",
            started.elapsed()
        );
    }
}
