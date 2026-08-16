use chrono::{Datelike, Timelike};
use std::collections::HashSet;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

/// Spawn a background task that checks once a minute whether the configured
/// weekly report is due. When due, it generates a Markdown report for the last
/// 7 days, writes it to disk, and fires a desktop notification.
pub fn spawn_weekly_report_scheduler(state: Arc<crate::routes::AppState>) {
    tokio::spawn(async move {
        // Keyed by ISO-8601 "year-week", e.g. "2025-07". We remember each week for
        // which a report has already been sent so we don't fire repeatedly within
        // the same week.
        let mut sent_weeks: HashSet<String> = HashSet::new();

        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;

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

            let now = chrono::Local::now();
            let weekday = now.weekday().number_from_monday(); // 1=Monday .. 7=Sunday
            let hour = now.hour();
            let minute = now.minute();
            let iso_year = now.iso_week().year();
            let iso_week = now.iso_week().week();
            let week_key = format!("{}-{:02}", iso_year, iso_week);

            let due = weekday == config.server.weekly_report_day
                && (hour, minute) >= (config.server.weekly_report_hour, config.server.weekly_report_minute);

            if !due || sent_weeks.contains(&week_key) {
                continue;
            }

            // Mark as sent before doing the work to avoid duplicate sends if the
            // report generation is slow and the 60s tick fires again.
            sent_weeks.insert(week_key.clone());

            if let Err(e) = run_weekly_report(&state, &now).await {
                log::warn!("Weekly report: generation failed: {}", e);
            }
        }
    });
}

async fn run_weekly_report(
    state: &Arc<crate::routes::AppState>,
    today_dt: &chrono::DateTime<chrono::Local>,
) -> anyhow::Result<()> {
    let to = today_dt.format("%Y-%m-%d").to_string();
    let from = (*today_dt - chrono::Duration::days(6)).format("%Y-%m-%d").to_string();

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

    // Write the report to ~/.local/share/hyprtrace/reports/weekly-<to>.md.
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    let reports_dir = std::path::PathBuf::from(home)
        .join(".local/share/hyprtrace/reports");
    std::fs::create_dir_all(&reports_dir)?;
    let path = reports_dir.join(format!("weekly-{}.md", to));
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

    let result = Command::new("notify-send")
        .args(["-a", "hyprtrace", "HyprTrace weekly report", &notification_body])
        .spawn();
    match result {
        Ok(_) => log::info!("Weekly report notification sent"),
        Err(e) => log::warn!("Failed to send weekly report notification: {}", e),
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
