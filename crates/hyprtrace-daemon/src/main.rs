mod config;
mod db;
mod disruption_monitor;
mod goal_monitor;
mod idle_monitor;
mod input_monitor;
mod listener;
mod resource_monitor;
mod wellbeing_monitor;

use anyhow::Context;
use std::sync::{Arc, Mutex};

/// Why the daemon main loop woke up. Controls the process exit code so that
/// systemd (`Restart=on-failure`) can tell a graceful stop from a crash.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ShutdownReason {
    /// SIGINT/SIGTERM received (or every signal sender went away): graceful exit.
    TerminationSignal,
    /// The Hyprland event listener thread ended (error or unexpected stop).
    /// Exiting non-zero lets systemd restart the daemon instead of leaving a
    /// process that runs but records nothing.
    ListenerTerminated { error: Option<String> },
}

impl ShutdownReason {
    fn describe(&self) -> String {
        match self {
            ShutdownReason::TerminationSignal => "termination signal".to_string(),
            ShutdownReason::ListenerTerminated { error: None } => {
                "Hyprland event listener stopped unexpectedly".to_string()
            }
            ShutdownReason::ListenerTerminated { error: Some(e) } => {
                format!("Hyprland event listener failed: {}", e)
            }
        }
    }
}

/// Graceful shutdown exits 0; a dead listener exits non-zero so that
/// `Restart=on-failure` re-launches the daemon (e.g. when it started before
/// the Hyprland session was ready).
fn exit_code(reason: &ShutdownReason) -> i32 {
    match reason {
        ShutdownReason::TerminationSignal => 0,
        ShutdownReason::ListenerTerminated { .. } => 1,
    }
}

/// Map the outcome of `WindowTracker::run` (including a panic, caught here so
/// a panicking listener still terminates the whole daemon) to the shutdown
/// reason reported to the main thread.
fn tracker_exit_reason(
    outcome: Result<anyhow::Result<()>, Box<dyn std::any::Any + Send>>,
) -> ShutdownReason {
    match outcome {
        Ok(Ok(())) => ShutdownReason::ListenerTerminated { error: None },
        Ok(Err(e)) => ShutdownReason::ListenerTerminated {
            error: Some(e.to_string()),
        },
        Err(payload) => ShutdownReason::ListenerTerminated {
            error: Some(panic_message(&payload)),
        },
    }
}

fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        format!("panicked: {}", s)
    } else if let Some(s) = payload.downcast_ref::<String>() {
        format!("panicked: {}", s)
    } else {
        "panicked: unknown payload".to_string()
    }
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_secs()
        .init();

    log::info!("HyprTrace daemon starting...");

    let cfg = config::Config::load().context("Failed to load config")?;
    cfg.ensure_db_dir().context("Failed to create database directory")?;

    let db_path = cfg.db_path_expanded();
    let db = db::Database::open(&db_path, cfg.daemon.focused_threshold_seconds).context("Failed to open database")?;
    // The migration writes to the shared DB while the server process may also
    // be writing at startup. Transient lock contention is retried with a short
    // backoff instead of exiting (which used to cause a systemd crash loop).
    let mut migration_attempts = 0u32;
    loop {
        match db.migrate() {
            Ok(()) => break,
            Err(e) if is_sqlite_busy(&e) && migration_attempts < 5 => {
                migration_attempts += 1;
                let delay = std::time::Duration::from_secs(2 * migration_attempts as u64);
                log::warn!(
                    "Database migration busy (attempt {migration_attempts}/5), retrying in {}s: {:#}",
                    delay.as_secs(),
                    e
                );
                std::thread::sleep(delay);
            }
            Err(e) => return Err(e).context("Database migration failed"),
        }
    }
    log::info!("Database ready: {:?}", db_path);

    let db = Arc::new(Mutex::new(db));

    let activity = idle_monitor::ActivityState::new();

    idle_monitor::spawn_idle_monitor(
        db.clone(),
        activity.clone(),
        cfg.daemon.idle_timeout_seconds,
        cfg.daemon.focused_threshold_seconds,
    );

    if cfg.daemon.enable_input_monitor {
        input_monitor::spawn_input_monitor(activity.clone());
    }

    resource_monitor::spawn_resource_monitor(db.clone(), 30);

    goal_monitor::spawn_goal_monitor(db.clone(), 300, cfg.daemon.break_after_minutes);

    wellbeing_monitor::spawn_wellbeing_monitor(
        db.clone(),
        300,
        cfg.daemon.late_night_start_hour,
        cfg.daemon.late_night_end_hour,
        cfg.daemon.hyprlock_command.clone(),
    );

    let _disruption_monitor = disruption_monitor::DisruptionMonitor::start(db.clone());

    let (tx, rx) = std::sync::mpsc::channel::<ShutdownReason>();
    let tx_signal = tx.clone();
    let db_shutdown = db.clone();
    ctrlc::set_handler(move || {
        log::info!("Received termination signal, shutting down gracefully...");

        // Retry the DB lock briefly instead of silently skipping on contention
        // (e.g. the idle monitor mid-tick). This is best-effort: during a fast
        // system reboot the signal may never be delivered, so orphaned sessions
        // are also finalized at the next daemon start.
        let mut guard = None;
        for _ in 0..20 {
            match db_shutdown.try_lock() {
                Ok(g) => {
                    guard = Some(g);
                    break;
                }
                Err(std::sync::TryLockError::WouldBlock) => {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                Err(std::sync::TryLockError::Poisoned(e)) => {
                    log::error!("DB mutex poisoned on shutdown, recovering: {}", e);
                    guard = Some(e.into_inner());
                    break;
                }
            }
        }
        if let Some(guard) = guard {
            match guard.current_session_id() {
                Ok(Some(id)) => match guard.end_session(id) {
                    Ok(()) => log::info!("Ended active session {} on shutdown", id),
                    Err(e) => log::error!("Failed to end active session {}: {}", id, e),
                },
                Ok(None) => {}
                Err(e) => log::error!("Failed to query current session on shutdown: {}", e),
            }
        } else {
            log::error!("Could not acquire DB lock on shutdown; session may linger");
        }

        tx_signal.send(ShutdownReason::TerminationSignal).ok();
    })?;

    let mut tracker = listener::WindowTracker::new(db, cfg, activity);
    let tx_tracker = tx;
    std::thread::spawn(move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| tracker.run()));
        let reason = tracker_exit_reason(outcome);
        log::error!("{}; shutting down daemon", reason.describe());
        tx_tracker.send(reason).ok();
    });

    // Wake up on either a termination signal or the listener thread dying;
    // otherwise the daemon would keep running while tracking nothing.
    let reason = rx.recv().unwrap_or(ShutdownReason::TerminationSignal);
    let code = exit_code(&reason);
    if code == 0 {
        log::info!("HyprTrace daemon exited");
        Ok(())
    } else {
        log::error!(
            "HyprTrace daemon exiting with code {} ({}) so systemd can restart it",
            code,
            reason.describe()
        );
        std::process::exit(code);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graceful_signal_exits_zero() {
        assert_eq!(exit_code(&ShutdownReason::TerminationSignal), 0);
    }

    #[test]
    fn listener_error_exits_nonzero_for_systemd_restart() {
        let reason = ShutdownReason::ListenerTerminated {
            error: Some("Failed to start Hyprland event listener!".to_string()),
        };
        assert_ne!(exit_code(&reason), 0);
    }

    #[test]
    fn listener_unexpected_clean_stop_also_exits_nonzero() {
        // Even without an error, a stopped listener means no tracking is
        // happening: the daemon must not linger as a zombie process.
        let reason = ShutdownReason::ListenerTerminated { error: None };
        assert_ne!(exit_code(&reason), 0);
    }

    #[test]
    fn tracker_run_error_maps_to_listener_terminated() {
        let reason = tracker_exit_reason(Ok(Err(anyhow::anyhow!("socket not found"))));
        match &reason {
            ShutdownReason::ListenerTerminated { error: Some(e) } => {
                assert!(e.contains("socket not found"), "unexpected message: {}", e);
            }
            other => panic!("unexpected reason: {:?}", other),
        }
        assert_ne!(exit_code(&reason), 0);
    }

    #[test]
    fn tracker_panic_maps_to_listener_terminated() {
        let payload: Box<dyn std::any::Any + Send> = Box::new("hyprland ipc gone".to_string());
        let reason = tracker_exit_reason(Err(payload));
        match &reason {
            ShutdownReason::ListenerTerminated { error: Some(e) } => {
                assert!(e.contains("hyprland ipc gone"), "unexpected message: {}", e);
            }
            other => panic!("unexpected reason: {:?}", other),
        }
        assert_ne!(exit_code(&reason), 0);
    }

    #[test]
    fn tracker_panic_with_str_payload_is_reported() {
        let payload: Box<dyn std::any::Any + Send> = Box::new("boom");
        let reason = tracker_exit_reason(Err(payload));
        match reason {
            ShutdownReason::ListenerTerminated { error: Some(e) } => {
                assert!(e.contains("boom"), "unexpected message: {}", e);
            }
            other => panic!("unexpected reason: {:?}", other),
        }
    }

    #[test]
    fn tracker_unexpected_clean_stop_maps_to_listener_terminated() {
        assert_eq!(
            tracker_exit_reason(Ok(Ok(()))),
            ShutdownReason::ListenerTerminated { error: None }
        );
    }

    #[test]
    fn closed_signal_channel_defaults_to_graceful_exit() {
        // Every sender gone (e.g. handler thread died before sending):
        // fall back to a graceful stop rather than blocking forever.
        let (tx, rx) = std::sync::mpsc::channel::<ShutdownReason>();
        drop(tx);
        let reason = rx.recv().unwrap_or(ShutdownReason::TerminationSignal);
        assert_eq!(exit_code(&reason), 0);
    }

    #[test]
    fn signal_sent_through_channel_yields_graceful_exit() {
        let (tx, rx) = std::sync::mpsc::channel::<ShutdownReason>();
        tx.send(ShutdownReason::TerminationSignal).unwrap();
        drop(tx);
        let reason = rx.recv().unwrap_or(ShutdownReason::TerminationSignal);
        assert_eq!(reason, ShutdownReason::TerminationSignal);
        assert_eq!(exit_code(&reason), 0);
    }
}

/// True when the error chain contains a SQLite busy/locked failure — transient
/// lock contention worth retrying with a backoff instead of crashing.
fn is_sqlite_busy(err: &anyhow::Error) -> bool {
    err.chain().any(|cause| {
        matches!(
            cause.downcast_ref::<rusqlite::Error>(),
            Some(rusqlite::Error::SqliteFailure(ffi, _))
                if ffi.code == rusqlite::ErrorCode::DatabaseBusy
                    || ffi.code == rusqlite::ErrorCode::DatabaseLocked
        )
    })
}
