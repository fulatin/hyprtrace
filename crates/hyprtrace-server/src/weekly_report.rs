use chrono::{Datelike, NaiveDateTime};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// Name of the marker file (inside the reports directory) that records the ISO
/// week of the last generated report. Persisting it — instead of only keeping
/// the `sent` state in memory — means a server restart cannot re-send a week's
/// report, while a missed window is still caught up after the fact.
const SENT_MARKER: &str = ".last-weekly-report";

/// Spawn a background task that checks once a minute whether the configured
/// weekly report is due. When due, it generates a Markdown report for the last
/// 7 days, writes it to disk, and fires a desktop notification.
pub fn spawn_weekly_report_scheduler(state: Arc<crate::routes::AppState>) {
    tokio::spawn(async move {
        // ISO-8601 week ("2026-45") of the report we last sent, seeded from
        // disk. A marker also prevents a duplicate send if the wall clock is
        // stepped backwards across the window.
        let mut sent_week: Option<String> = read_sent_marker();
        // Wall-clock time of the previous tick. The due check asks "did the
        // configured window fall into the interval since the last tick?"
        // instead of "is it exactly the configured weekday *now*", which is
        // what makes the scheduler survive suspend (issue #30).
        let mut last_tick = chrono::Local::now();

        loop {
            // `tokio::time::sleep` is based on CLOCK_MONOTONIC, which does not
            // advance while the machine is suspended: after a 3-hour lid-close
            // this fires within a minute of wake-up. The interval between the
            // last tick and the wake-up tick then contains the slept-through
            // window, and `scheduled_between` still sees it.
            tokio::time::sleep(Duration::from_secs(60)).await;

            let now = chrono::Local::now();
            let from = last_tick.naive_local();
            let to = now.naive_local();
            last_tick = now;

            // Snapshot the config without panicking if it's locked/unavailable.
            let config = match state.config.try_lock() {
                Ok(guard) => guard.clone(),
                Err(_) => {
                    log::debug!("Weekly report: config lock busy, skipping tick");
                    continue;
                }
            };

            if !config.server.weekly_report_enabled {
                continue;
            }

            let Some(due_at) = scheduled_between(
                from,
                to,
                config.server.weekly_report_day,
                config.server.weekly_report_hour,
                config.server.weekly_report_minute,
            ) else {
                continue;
            };

            // Dedupe by the week the window belongs to (not the week of the
            // tick): a window that crosses midnight still counts once.
            let due_week = week_key(due_at);
            if sent_week.as_deref() == Some(due_week.as_str()) {
                continue;
            }

            // Mark before doing the work to avoid a duplicate send if report
            // generation outlasts the tick; a failure clears it so the next
            // tick retries.
            sent_week = Some(due_week.clone());

            match run_weekly_report(&state, &now).await {
                Ok(()) => write_sent_marker(&due_week),
                Err(e) => {
                    log::warn!("Weekly report: generation failed: {}", e);
                    sent_week = None;
                }
            }
        }
    });
}

/// The configured send time, if it falls in the half-open interval `(from, to]`.
///
/// This is a calendar-window test rather than a "is now the configured minute"
/// test: whenever the scheduler wakes up late (suspend/resume, a stalled
/// runtime, a clock step forward) the window it slept through is still inside
/// the interval and the report fires exactly once.
///
/// Out-of-range config values are treated as "disabled" so a hand-edited
/// config cannot fire at an unexpected time.
fn scheduled_between(
    from: NaiveDateTime,
    to: NaiveDateTime,
    day: u32,
    hour: u32,
    minute: u32,
) -> Option<NaiveDateTime> {
    if !(1..=7).contains(&day) || hour > 23 || minute > 59 || to <= from {
        return None;
    }
    let mut date = from.date();
    let last = to.date();
    while date <= last {
        if date.weekday().number_from_monday() == day {
            let candidate = date.and_hms_opt(hour, minute, 0)?;
            if candidate > from && candidate <= to {
                return Some(candidate);
            }
        }
        date = date.succ_opt()?;
    }
    None
}

/// ISO-8601 year-week key, e.g. `2026-45`, of the day the report is for. Used
/// to deduplicate reports within one week.
fn week_key(at: NaiveDateTime) -> String {
    let week = at.date().iso_week();
    format!("{}-{:02}", week.year(), week.week())
}

/// `~/.local/share/hyprtrace/reports`, where the Markdown reports and the
/// sent-marker live.
fn reports_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    PathBuf::from(home).join(".local/share/hyprtrace/reports")
}

fn marker_path() -> PathBuf {
    reports_dir().join(SENT_MARKER)
}

fn read_sent_marker() -> Option<String> {
    let raw = std::fs::read_to_string(marker_path()).ok()?;
    let trimmed = raw.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn write_sent_marker(week: &str) {
    let dir = reports_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        log::warn!("Weekly report: cannot create reports dir: {}", e);
        return;
    }
    if let Err(e) = std::fs::write(marker_path(), format!("{week}\n")) {
        log::warn!("Weekly report: cannot persist the sent marker: {}", e);
    }
}

async fn run_weekly_report(
    state: &Arc<crate::routes::AppState>,
    today_dt: &chrono::DateTime<chrono::Local>,
) -> anyhow::Result<()> {
    let to = today_dt.format("%Y-%m-%d").to_string();
    let from = (*today_dt - chrono::Duration::days(6))
        .format("%Y-%m-%d")
        .to_string();

    let (report_md, total_ms, top_class, top_ms) = {
        let db = state.db.lock().await;
        let report = db.report(&from, &to)?;
        let (total, top) = db.weekly_totals(&from, &to)?;
        let top_ms = db
            .app_ranking(&from, &to, 1)?
            .into_iter()
            .next()
            .map(|a| a.total_ms)
            .unwrap_or(0);
        (report, total, top, top_ms)
    };

    let report_dir = reports_dir();
    std::fs::create_dir_all(&report_dir)?;
    let path = report_dir.join(format!("weekly-{}.md", to));
    std::fs::write(&path, report_md)?;
    log::info!("Weekly report written to {:?}", path);

    // Compact human-readable totals for the notification body.
    let total_str = format_duration(total_ms);
    let notification_body = if top_class.is_empty() {
        format!("Week total: {} · Report: {}", total_str, path.display())
    } else {
        format!(
            "Week total: {} · Top app: {} ({}) · Report: {}",
            total_str,
            top_class,
            format_duration(top_ms),
            path.display()
        )
    };

    // D-Bus notification on a blocking thread (the zbus call is synchronous);
    // no dependency on the `notify-send` binary being installed.
    let body = notification_body.clone();
    let sent = tokio::task::spawn_blocking(move || {
        hyprtrace_common::notify::send("HyprTrace weekly report", &body);
    })
    .await;
    match sent {
        Ok(()) => log::info!("Weekly report notification sent: {}", notification_body),
        Err(e) => log::warn!("Weekly report notification task failed: {}", e),
    }

    Ok(())
}

/// Format milliseconds as a compact `34h 12m` (or `12m` / `45s`) string.
fn format_duration(ms: i64) -> String {
    if ms <= 0 {
        return "0m".to_string();
    }
    let total_secs = ms / 1000;
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else if minutes > 0 {
        format!("{}m", minutes)
    } else {
        format!("{}s", total_secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn at(y: i32, m: u32, d: u32, h: u32, min: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(h, min, 0)
            .unwrap()
    }

    #[test]
    fn fires_when_the_window_falls_inside_the_tick_interval() {
        // Monday 2026-11-09, configured Monday 09:00: the tick spanning
        // 08:59 -> 09:00 must fire, and the one ending just before must not.
        assert_eq!(
            scheduled_between(at(2026, 11, 9, 8, 59), at(2026, 11, 9, 9, 0), 1, 9, 0),
            Some(at(2026, 11, 9, 9, 0))
        );
        assert_eq!(
            scheduled_between(at(2026, 11, 9, 8, 58), at(2026, 11, 9, 8, 59), 1, 9, 0),
            None
        );
    }

    #[test]
    fn catches_up_a_window_slept_through() {
        // Issue #30: suspended from Monday 22:00 to Tuesday 01:00 with the
        // report configured for Monday 23:00. A point-in-time "is it Monday
        // now?" check skipped that week entirely; the interval check still
        // sees Monday 23:00.
        assert_eq!(
            scheduled_between(at(2026, 11, 9, 22, 0), at(2026, 11, 10, 1, 0), 1, 23, 0),
            Some(at(2026, 11, 9, 23, 0))
        );
        // A 3-hour lid-close that stays on the configured day (the acceptance
        // example) is caught the same way.
        assert_eq!(
            scheduled_between(at(2026, 11, 9, 8, 0), at(2026, 11, 9, 11, 0), 1, 9, 0),
            Some(at(2026, 11, 9, 9, 0))
        );
    }

    #[test]
    fn a_window_outside_the_interval_does_not_fire() {
        // Wrong weekday.
        assert_eq!(
            scheduled_between(at(2026, 11, 10, 8, 0), at(2026, 11, 10, 10, 0), 1, 9, 0),
            None
        );
        // Clock stepped backwards: `to <= from` never fires.
        assert_eq!(
            scheduled_between(at(2026, 11, 9, 10, 0), at(2026, 11, 9, 8, 0), 1, 9, 0),
            None
        );
    }

    #[test]
    fn invalid_schedule_is_disabled() {
        for (day, hour, minute) in [(0u32, 9u32, 0u32), (8, 9, 0), (1, 24, 0), (1, 9, 60)] {
            assert_eq!(
                scheduled_between(
                    at(2026, 11, 9, 8, 0),
                    at(2026, 11, 9, 10, 0),
                    day,
                    hour,
                    minute
                ),
                None,
                "day={day} hour={hour} minute={minute} must be treated as disabled"
            );
        }
    }

    #[test]
    fn week_key_uses_the_window_day_not_the_tick_day() {
        // A window on Sunday 2026-11-08 23:00 is ISO week 45 even when the
        // tick that fires it lands after midnight (Monday, week 46), so the
        // dedupe marker still blocks a second send for that window.
        assert_eq!(week_key(at(2026, 11, 8, 23, 0)), "2026-45");
        assert_eq!(week_key(at(2026, 11, 9, 9, 0)), "2026-46");
    }
}
