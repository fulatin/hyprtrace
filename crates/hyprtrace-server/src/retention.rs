use std::sync::Arc;
use std::time::Duration;

/// Periodically enforce the configured retention policy: every 6 hours,
/// delete finished sessions older than `server.retention_days` days
/// (disabled when the value is 0).
pub fn spawn_retention_cleanup(state: Arc<crate::routes::AppState>) {
    tokio::spawn(async move {
        let interval = Duration::from_secs(6 * 60 * 60);

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

    let cutoff = (chrono::Utc::now() - chrono::Duration::days(retention_days as i64))
        .format("%Y-%m-%d")
        .to_string();

    let deleted = {
        let db = state.db.lock().await;
        match db.delete_sessions_before(&cutoff) {
            Ok(n) => n,
            Err(e) => {
                log::warn!("Retention cleanup failed: {}", e);
                return;
            }
        }
    };

    log::info!(
        "Retention cleanup: deleted {} session(s) started before {}",
        deleted,
        cutoff
    );
}
