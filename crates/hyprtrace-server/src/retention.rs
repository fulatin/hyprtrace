use std::sync::Arc;
use std::time::Duration;

/// Periodically enforce the configured retention policy: every 6 hours, delete
/// everything older than `server.retention_days` days — finished sessions with
/// their children, plus the auxiliary tables (`disruptions`, `app_resources`,
/// `activity_events`) that a session deletion does not always reach. Disabled
/// when the value is 0, which is the default: nothing is pruned, including the
/// raw resource samples that accumulate at one row per 30 s per session.
pub fn spawn_retention_cleanup(state: Arc<crate::routes::AppState>) {
    tokio::spawn(async move {
        let initial_delay = Duration::from_secs(5 * 60);
        let interval = Duration::from_secs(6 * 60 * 60);

        // Give the server time to boot and serve requests before the first
        // (potentially expensive) cleanup pass kicks in.
        tokio::time::sleep(initial_delay).await;

        loop {
            cleanup_once(&state).await;
            tokio::time::sleep(interval).await;
        }
    });
}

/// Runs one cleanup pass. Reads the current retention_days value, computes the
/// UTC cutoff date, and deletes everything strictly older than it.
async fn cleanup_once(state: &Arc<crate::routes::AppState>) {
    let retention_days = {
        let config = state.config.lock().await;
        config.server.retention_days
    };

    if retention_days == 0 {
        log::debug!("Retention cleanup disabled (retention_days = 0)");
        return;
    }

    let cutoff = (chrono::Local::now() - chrono::Duration::days(retention_days as i64))
        .format("%Y-%m-%d")
        .to_string();

    log::info!("Retention cleanup starting: cutoff date {}", cutoff);

    let outcome = {
        let db = state.db.lock().await;
        match db.apply_retention(&cutoff) {
            Ok(o) => o,
            Err(e) => {
                log::warn!("Retention cleanup failed: {}", e);
                return;
            }
        }
    };

    log::info!(
        "Retention cleanup before {}: {} session(s), {} disruption(s), {} resource sample(s), {} activity event(s)",
        cutoff,
        outcome.sessions,
        outcome.disruptions,
        outcome.app_resources,
        outcome.activity_events
    );
}
