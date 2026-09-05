use crate::db::{Database, Goal};
use std::collections::HashSet;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// A goal progress milestone worth notifying about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Milestone {
    Half,
    Done,
}

/// Decide which milestone notification (if any) to fire for `goal`, and mark
/// it as notified. The dedup key includes the calendar date so a long-running
/// daemon re-arms the 50%/100% notifications every day — without the date,
/// they only ever fired once per goal per daemon lifetime.
fn milestone_to_notify(
    notified: &mut HashSet<(String, String, String)>,
    date: &str,
    goal: &Goal,
    pct: f64,
) -> Option<Milestone> {
    let key = goal_key(goal);
    if pct >= 100.0 && !notified.contains(&(date.to_string(), key.clone(), "done".into())) {
        notified.insert((date.to_string(), key, "done".into()));
        Some(Milestone::Done)
    } else if pct >= 50.0 && !notified.contains(&(date.to_string(), key.clone(), "half".into())) {
        notified.insert((date.to_string(), key, "half".into()));
        Some(Milestone::Half)
    } else {
        None
    }
}

/// Periodically checks daily goals and focused-session duration, firing desktop
/// notifications at 50% / 100% goal progress and after long focused stretches.
pub fn spawn_goal_monitor(
    db: Arc<Mutex<Database>>,
    check_interval_secs: u64,
    break_after_minutes: u64,
) {
    std::thread::spawn(move || {
        let interval = Duration::from_secs(check_interval_secs.max(60));
        // Dedup keys: (local date, goal key, milestone). The date component
        // re-arms notifications each new day for the daemon's lifetime.
        let mut notified: HashSet<(String, String, String)> = HashSet::new();
        let mut break_notified: Option<(String, bool)> = None;

        loop {
            std::thread::sleep(interval);

            let guard = match db.lock() {
                Ok(g) => g,
                Err(_) => continue,
            };

            let goals = match guard.goals() {
                Ok(g) => g,
                Err(_) => continue,
            };

            let today = chrono::Local::now().format("%Y-%m-%d").to_string();
            // Only today's keys are ever relevant; drop stale days so the
            // set cannot grow without bound over a long daemon lifetime.
            notified.retain(|(d, _, _)| d == &today);

            for goal in goals {
                if !goal.enabled {
                    continue;
                }
                let today_ms = guard.today_active_for_goal(&goal);
                let pct = if goal.daily_target_ms > 0 {
                    (today_ms as f64 / goal.daily_target_ms as f64) * 100.0
                } else {
                    0.0
                };
                match milestone_to_notify(&mut notified, &today, &goal, pct) {
                    Some(Milestone::Done) => send_notify(
                        "Goal achieved",
                        &format!(
                            "{}: reached 100% of today's target ({} / {})",
                            goal.name,
                            fmt_ms(today_ms),
                            fmt_ms(goal.daily_target_ms)
                        ),
                    ),
                    Some(Milestone::Half) => send_notify(
                        "Halfway there",
                        &format!(
                            "{}: 50% of today's target reached ({} / {})",
                            goal.name,
                            fmt_ms(today_ms),
                            fmt_ms(goal.daily_target_ms)
                        ),
                    ),
                    None => {}
                }
            }

            // Focus break reminder. Local date: same daily-bucket semantics as
            // the summary tables (see M3).
            let focused_ms = guard.current_focused_duration_ms();
            let day = chrono::Local::now().format("%Y-%m-%d").to_string();
            let focused_min = focused_ms / 60_000;
            if break_after_minutes > 0 && focused_min as u64 >= break_after_minutes {
                if break_notified.as_ref().map(|(d, _)| d.as_str()) != Some(day.as_str()) {
                    send_notify(
                        "Take a break",
                        &format!(
                            "You've been focused on one window for {} min. Stretch and look away for a moment.",
                            focused_min
                        ),
                    );
                    break_notified = Some((day, true));
                }
            }
        }
    });
}

fn goal_key(goal: &Goal) -> String {
    format!(
        "{}:{}:{}",
        goal.name,
        goal.target_type,
        goal.target_key.as_deref().unwrap_or("")
    )
}

fn fmt_ms(ms: i64) -> String {
    let hours = ms / 3_600_000;
    let mins = (ms % 3_600_000) / 60_000;
    if hours > 0 {
        format!("{}h {}m", hours, mins)
    } else {
        format!("{}m", mins)
    }
}

fn send_notify(summary: &str, body: &str) {
    log::info!("Reminder: {} - {}", summary, body);
    if let Err(e) = Command::new("notify-send")
        .args(["-a", "hyprtrace", summary, body])
        .spawn()
    {
        log::warn!("notify-send failed: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn goal(name: &str) -> Goal {
        Goal {
            id: Some(1),
            name: name.to_string(),
            target_type: "all".to_string(),
            target_key: None,
            daily_target_ms: 3_600_000,
            enabled: true,
        }
    }

    #[test]
    fn first_halfway_crossing_notifies_once() {
        let mut notified = HashSet::new();
        assert_eq!(
            milestone_to_notify(&mut notified, "2026-09-05", &goal("code"), 55.0),
            Some(Milestone::Half)
        );
        // Same day, still >= 50%: deduplicated.
        assert_eq!(
            milestone_to_notify(&mut notified, "2026-09-05", &goal("code"), 60.0),
            None
        );
    }

    #[test]
    fn done_fires_after_half_on_same_day() {
        let mut notified = HashSet::new();
        assert_eq!(
            milestone_to_notify(&mut notified, "2026-09-05", &goal("code"), 50.0),
            Some(Milestone::Half)
        );
        assert_eq!(
            milestone_to_notify(&mut notified, "2026-09-05", &goal("code"), 100.0),
            Some(Milestone::Done)
        );
        // Both milestones fired for today: nothing more.
        assert_eq!(
            milestone_to_notify(&mut notified, "2026-09-05", &goal("code"), 130.0),
            None
        );
    }

    #[test]
    fn jump_to_done_then_half_fires_late() {
        // Quirk preserved from the original code: when progress jumps from
        // <50% straight to 100%, "done" fires first and "half" fires on the
        // next check via the else-if branch. Only after both is it silent.
        let mut notified = HashSet::new();
        assert_eq!(
            milestone_to_notify(&mut notified, "2026-09-05", &goal("code"), 100.0),
            Some(Milestone::Done)
        );
        assert_eq!(
            milestone_to_notify(&mut notified, "2026-09-05", &goal("code"), 100.0),
            Some(Milestone::Half)
        );
        assert_eq!(
            milestone_to_notify(&mut notified, "2026-09-05", &goal("code"), 100.0),
            None
        );
    }

    #[test]
    fn below_half_never_notifies() {
        let mut notified = HashSet::new();
        assert_eq!(
            milestone_to_notify(&mut notified, "2026-09-05", &goal("code"), 49.9),
            None
        );
    }

    #[test]
    fn new_day_rearms_notifications() {
        // Regression test for M1: without the date in the dedup key, a
        // long-running daemon stopped notifying from the second day on.
        let mut notified = HashSet::new();
        assert_eq!(
            milestone_to_notify(&mut notified, "2026-09-05", &goal("code"), 50.0),
            Some(Milestone::Half)
        );
        assert_eq!(
            milestone_to_notify(&mut notified, "2026-09-05", &goal("code"), 100.0),
            Some(Milestone::Done)
        );
        // Next day, progress accumulates from scratch: both fire again.
        assert_eq!(
            milestone_to_notify(&mut notified, "2026-09-06", &goal("code"), 50.0),
            Some(Milestone::Half)
        );
        assert_eq!(
            milestone_to_notify(&mut notified, "2026-09-06", &goal("code"), 100.0),
            Some(Milestone::Done)
        );
    }

    #[test]
    fn new_day_still_over_100_percent_notifies_again() {
        // Even if the measured total (e.g. stale stats) is already >= 100% on
        // the next day, the fresh date key allows the notification through.
        let mut notified = HashSet::new();
        assert_eq!(
            milestone_to_notify(&mut notified, "2026-09-05", &goal("code"), 120.0),
            Some(Milestone::Done)
        );
        assert_eq!(
            milestone_to_notify(&mut notified, "2026-09-06", &goal("code"), 120.0),
            Some(Milestone::Done)
        );
    }

    #[test]
    fn different_goals_do_not_dedup_each_other() {
        let mut notified = HashSet::new();
        assert_eq!(
            milestone_to_notify(&mut notified, "2026-09-05", &goal("code"), 50.0),
            Some(Milestone::Half)
        );
        assert_eq!(
            milestone_to_notify(&mut notified, "2026-09-05", &goal("read"), 50.0),
            Some(Milestone::Half)
        );
    }

    #[test]
    fn retain_drops_other_days_keys() {
        // The monitor loop prunes stale dates so the set stays bounded.
        let mut notified: HashSet<(String, String, String)> = HashSet::new();
        notified.insert(("2026-09-04".into(), "code:all:".into(), "done".into()));
        notified.insert(("2026-09-05".into(), "code:all:".into(), "half".into()));
        let today = "2026-09-05".to_string();
        notified.retain(|(d, _, _)| d == &today);
        assert_eq!(notified.len(), 1);
        assert!(notified.contains(&("2026-09-05".into(), "code:all:".into(), "half".into())));
    }
}
