use crate::db::{Database, Goal};
use std::collections::HashSet;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Periodically checks daily goals and focused-session duration, firing desktop
/// notifications at 50% / 100% goal progress and after long focused stretches.
pub fn spawn_goal_monitor(
    db: Arc<Mutex<Database>>,
    check_interval_secs: u64,
    break_after_minutes: u64,
) {
    std::thread::spawn(move || {
        let interval = Duration::from_secs(check_interval_secs.max(60));
        // Track which notification thresholds we already fired for a goal today.
        let mut notified: HashSet<(String, String)> = HashSet::new();
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
                let key = goal_key(&goal);
                if pct >= 100.0 && !notified.contains(&(key.clone(), "done".into())) {
                    send_notify(
                        "Goal achieved",
                        &format!(
                            "{}: reached 100% of today's target ({} / {})",
                            goal.name,
                            fmt_ms(today_ms),
                            fmt_ms(goal.daily_target_ms)
                        ),
                    );
                    notified.insert((key.clone(), "done".into()));
                } else if pct >= 50.0 && !notified.contains(&(key.clone(), "half".into())) {
                    send_notify(
                        "Halfway there",
                        &format!(
                            "{}: 50% of today's target reached ({} / {})",
                            goal.name,
                            fmt_ms(today_ms),
                            fmt_ms(goal.daily_target_ms)
                        ),
                    );
                    notified.insert((key, "half".into()));
                }
            }

            // Focus break reminder.
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
