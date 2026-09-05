use crate::db::Database;
use chrono::Timelike;
use std::collections::HashSet;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Digital-wellbeing nudges: reminds the user when they're active late at night
/// and (optionally) can trigger Hyprlock to enforce a wind-down time.
pub fn spawn_wellbeing_monitor(
    db: Arc<Mutex<Database>>,
    check_interval_secs: u64,
    late_night_start_hour: u32,
    late_night_end_hour: u32,
    hyprlock_command: Option<String>,
) {
    std::thread::spawn(move || {
        let interval = Duration::from_secs(check_interval_secs.max(60));
        let mut reminded_days: HashSet<String> = HashSet::new();

        loop {
            std::thread::sleep(interval);

            let now = chrono::Local::now();
            let hour = now.hour();
            let day = now.format("%Y-%m-%d").to_string();

            // Only act inside the late-night window.
            let in_window = if late_night_start_hour <= late_night_end_hour {
                hour >= late_night_start_hour && hour < late_night_end_hour
            } else {
                // Window crosses midnight (e.g. 23:00 → 06:00).
                hour >= late_night_start_hour || hour < late_night_end_hour
            };
            if !in_window {
                reminded_days.clear();
                continue;
            }

            // Is the user actually active right now? Check for an open session.
            let active = match db.lock() {
                Ok(g) => g.current_session_id().ok().flatten().is_some(),
                Err(_) => false,
            };
            if !active {
                continue;
            }

            let key = format!("{}:{}", day, hour);
            if reminded_days.contains(&key) {
                continue;
            }
            reminded_days.insert(key);

            send_notify(
                "Late night? 🌙",
                &format!(
                    "It's {}. You've been active late — consider winding down. Rest improves tomorrow's focus.",
                    now.format("%H:%M")
                ),
            );

            // Optional hard cutoff: lock the session.
            if let Some(cmd) = &hyprlock_command {
                log::info!("Digital wellbeing: triggering Hyprlock cutoff");
                // Execute directly WITHOUT a shell (security review H1):
                // the command comes from the config file, so shell
                // metacharacters must never be interpreted. The value is
                // expected to be a program name followed by plain arguments,
                // e.g. "hyprlock" or "swaylock -f".
                let mut parts = cmd.split_whitespace();
                match (parts.next(), parts.collect::<Vec<_>>()) {
                    (Some(program), args) => {
                        if let Err(e) = Command::new(program).args(&args).spawn() {
                            log::warn!("Failed to spawn {:?}: {}", program, e);
                        }
                    }
                    (None, _) => {
                        log::warn!("hyprlock_command is empty, skipping cutoff");
                    }
                }
            }
        }
    });
}

fn send_notify(summary: &str, body: &str) {
    log::info!("Wellbeing reminder: {}", body);
    if let Err(e) = Command::new("notify-send")
        .args(["-a", "hyprtrace", "-u", "critical", summary, body])
        .spawn()
    {
        log::warn!("notify-send failed: {}", e);
    }
}
