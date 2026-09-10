//! The one definition of "focus time" in the application.
//!
//! A focus block is a run of consecutive sessions of the SAME class, on the same
//! local date, whose summed dwell reaches [`FOCUS_BLOCK_MIN_MS`]. The focus time
//! of a block is that summed dwell — time actually spent in one context, not the
//! wall-clock span of the run, so an idle gap inside a run cannot inflate it.
//!
//! Why this replaced the per-session threshold: `sessions.focused_ms` was
//! `max(0, duration_ms - focused_threshold_ms)`, and with a median session of
//! about a second nothing can ever reach a 20-minute per-session threshold. On
//! this machine's real data (618 sessions in a day, median dwell 1126 ms) the
//! figure was therefore always zero — the dashboard showed "Focus time <1m" and
//! the 40-point focus component of the efficiency score was dead. The block rule
//! needs no per-session minimum, so it measures what the user actually did.
//!
//! Everything that reports focus goes through this module: the efficiency score,
//! the per-app and per-hour breakdowns and the fragmentation insight. That is
//! deliberate — before it, the same day had two answers depending on which code
//! path you asked.

use super::InsightSession;
use std::collections::HashMap;

/// Minimum summed dwell for a run of same-class sessions to count as focus.
pub const FOCUS_BLOCK_MIN_MS: i64 = 15 * 60_000;

/// A run of same-class sessions that adds up to real focus time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusBlock {
    /// Local calendar date the run belongs to.
    pub date: String,
    pub class: String,
    /// Summed dwell of the member sessions — the block's focus time.
    pub duration_ms: i64,
    pub session_count: i64,
}

/// Walk the runs once, returning the blocks and, for each input session,
/// whether it belongs to one.
///
/// `sessions` must be ordered by start time, which is what
/// `Database::insight_sessions` returns; runs never merge across a local date.
fn scan(sessions: &[&InsightSession]) -> (Vec<FocusBlock>, Vec<bool>) {
    let mut blocks = Vec::new();
    let mut members = vec![false; sessions.len()];
    let mut i = 0;
    while i < sessions.len() {
        let start = i;
        let date = sessions[i].date_local.as_str();
        let class = sessions[i].class.as_str();
        let mut run_ms = 0i64;
        while i < sessions.len()
            && sessions[i].date_local == date
            && sessions[i].class == class
        {
            run_ms += sessions[i].dwell_ms();
            i += 1;
        }
        if run_ms >= FOCUS_BLOCK_MIN_MS {
            members[start..i].fill(true);
            blocks.push(FocusBlock {
                date: date.to_string(),
                class: class.to_string(),
                duration_ms: run_ms,
                session_count: (i - start) as i64,
            });
        }
    }
    (blocks, members)
}

fn scan_owned(sessions: &[InsightSession]) -> (Vec<FocusBlock>, Vec<bool>) {
    let refs: Vec<&InsightSession> = sessions.iter().collect();
    scan(&refs)
}

/// The focus blocks in a slice of borrowed sessions — how the day-grouped
/// insights hold their rows, so they do not have to clone them.
///
/// This is the only block-list entry point on purpose: an owned-slice wrapper
/// that nothing but the tests called would be dead code in a real build.
pub fn blocks_of(sessions: &[&InsightSession]) -> Vec<FocusBlock> {
    scan(sessions).0
}

/// Total focus time in `sessions`, in milliseconds.
pub fn focus_total_ms(sessions: &[InsightSession]) -> i64 {
    scan_owned(sessions).0.iter().map(|b| b.duration_ms).sum()
}

/// What one class contributed to focus time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ClassFocus {
    /// Focus time from this class's blocks.
    pub focus_ms: i64,
    /// How many of the class's sessions were part of a block — the honest
    /// reading of "focused sessions", as opposed to the number of sessions that
    /// happened to be long enough on their own.
    pub sessions: i64,
}

/// Focus per class, for the classes that have any.
pub fn focus_by_class(sessions: &[InsightSession]) -> HashMap<String, ClassFocus> {
    let mut out: HashMap<String, ClassFocus> = HashMap::new();
    for block in scan_owned(sessions).0 {
        let entry = out.entry(block.class).or_default();
        entry.focus_ms += block.duration_ms;
        entry.sessions += block.session_count;
    }
    out
}

/// Focus time per local hour of the day.
///
/// Each member session contributes its whole dwell to the hour it STARTED in,
/// matching how every other hourly figure in the application buckets a session
/// (see `sessions.date_local` and the hourly summary).
pub fn focus_ms_by_hour(sessions: &[InsightSession]) -> HashMap<u8, i64> {
    use chrono::Timelike;

    let (_, members) = scan_owned(sessions);
    let mut out: HashMap<u8, i64> = HashMap::new();
    for (session, is_member) in sessions.iter().zip(members) {
        if !is_member {
            continue;
        }
        let Some(local) = chrono::DateTime::parse_from_rfc3339(&session.started_at)
            .ok()
            .map(|dt| dt.with_timezone(&chrono::Local))
        else {
            continue;
        };
        *out.entry(local.hour() as u8).or_insert(0) += session.dwell_ms().max(0);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::insights::test_support::session;

    /// `blocks_of` takes references, which is how the insights hold their rows.
    fn blocks(sessions: &[InsightSession]) -> Vec<FocusBlock> {
        let refs: Vec<&InsightSession> = sessions.iter().collect();
        blocks_of(&refs)
    }

    const MIN: i64 = 5 * 60_000;

    /// Five-minute sessions: three of them make a block, two do not.
    #[test]
    fn a_run_reaches_the_threshold_only_when_it_adds_up() {
        let short = vec![
            session("kitty", "2025-03-04T10:00:00Z", "2025-03-04T10:05:00Z", MIN, "2025-03-04"),
            session("kitty", "2025-03-04T10:05:00Z", "2025-03-04T10:10:00Z", MIN, "2025-03-04"),
        ];
        assert!(blocks(&short).is_empty(), "two runs of 5 min are not 15");

        let mut long = short.clone();
        long.push(session(
            "kitty",
            "2025-03-04T10:10:00Z",
            "2025-03-04T10:15:00Z",
            MIN,
            "2025-03-04",
        ));
        let found = blocks(&long);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].duration_ms, FOCUS_BLOCK_MIN_MS);
        assert_eq!(found[0].session_count, 3);
        assert_eq!(found[0].class, "kitty");
        assert_eq!(focus_total_ms(&long), FOCUS_BLOCK_MIN_MS);
    }

    /// A different class in the middle ends the run. The two kitty runs add up
    /// to 20 minutes between them, but they are not contiguous, so only
    /// firefox's unbroken 20 minutes is a block.
    #[test]
    fn another_class_breaks_the_run() {
        let sessions = vec![
            session("kitty", "2025-03-04T10:00:00Z", "2025-03-04T10:05:00Z", MIN, "2025-03-04"),
            session("kitty", "2025-03-04T10:05:00Z", "2025-03-04T10:10:00Z", MIN, "2025-03-04"),
            session("firefox", "2025-03-04T10:10:00Z", "2025-03-04T10:20:00Z", 600_000, "2025-03-04"),
            session("firefox", "2025-03-04T10:20:00Z", "2025-03-04T10:30:00Z", 600_000, "2025-03-04"),
            session("kitty", "2025-03-04T10:30:00Z", "2025-03-04T10:35:00Z", MIN, "2025-03-04"),
            session("kitty", "2025-03-04T10:35:00Z", "2025-03-04T10:40:00Z", MIN, "2025-03-04"),
        ];
        let found = blocks(&sessions);
        assert_eq!(found.len(), 1, "only firefox's run is unbroken");
        assert_eq!(found[0].class, "firefox");
        assert_eq!(found[0].duration_ms, 1_200_000);
        assert_eq!(found[0].session_count, 2);
    }

    /// A run never merges across the local date boundary, the same way every
    /// other figure in the application buckets by the date a session started.
    #[test]
    fn midnight_breaks_the_run() {
        let sessions = vec![
            session("kitty", "2025-03-04T23:50:00Z", "2025-03-04T23:55:00Z", MIN, "2025-03-04"),
            session("kitty", "2025-03-04T23:55:00Z", "2025-03-05T00:00:00Z", MIN, "2025-03-04"),
            session("kitty", "2025-03-05T00:00:00Z", "2025-03-05T00:05:00Z", MIN, "2025-03-05"),
            session("kitty", "2025-03-05T00:05:00Z", "2025-03-05T00:10:00Z", MIN, "2025-03-05"),
        ];
        assert!(
            blocks(&sessions).is_empty(),
            "two runs of 10 minutes, one on each side of midnight"
        );
    }

    #[test]
    fn the_aggregates_agree_with_the_blocks() {
        let sessions = vec![
            session("kitty", "2025-03-04T10:00:00Z", "2025-03-04T10:20:00Z", 1_200_000, "2025-03-04"),
            session("firefox", "2025-03-04T14:00:00Z", "2025-03-04T14:20:00Z", 1_200_000, "2025-03-04"),
            // A one-second session never contributes on its own.
            session("zed", "2025-03-04T15:00:00Z", "2025-03-04T15:00:01Z", 1_000, "2025-03-04"),
        ];
        assert_eq!(focus_total_ms(&sessions), 2_400_000);
        let by_class = focus_by_class(&sessions);
        assert_eq!(by_class.get("kitty").map(|f| f.focus_ms), Some(1_200_000));
        assert_eq!(by_class.get("firefox").map(|f| f.focus_ms), Some(1_200_000));
        assert_eq!(by_class.get("zed"), None);
        assert_eq!(
            by_class.get("kitty").map(|f| f.sessions),
            Some(1),
            "one session of kitty was long enough to be its own block"
        );

        let by_hour = focus_ms_by_hour(&sessions);
        assert_eq!(by_hour.len(), 2, "the zed session contributes nothing");
        assert_eq!(by_hour.values().sum::<i64>(), 2_400_000);
    }
}
