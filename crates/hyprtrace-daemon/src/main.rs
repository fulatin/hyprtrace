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

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_secs()
        .init();

    log::info!("HyprTrace daemon starting...");

    let cfg = config::Config::load().context("Failed to load config")?;
    cfg.ensure_db_dir().context("Failed to create database directory")?;

    let db_path = cfg.db_path_expanded();
    let db = db::Database::open(&db_path, cfg.daemon.focused_threshold_seconds).context("Failed to open database")?;
    db.migrate().context("Database migration failed")?;
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

    let (tx, rx) = std::sync::mpsc::channel();
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

        tx.send(()).ok();
    })?;

    let mut tracker = listener::WindowTracker::new(db, cfg, activity);
    std::thread::spawn(move || {
        if let Err(e) = tracker.run() {
            log::error!("Window tracker exited with error: {}", e);
        }
    });

    rx.recv().ok();
    log::info!("HyprTrace daemon exited");
    Ok(())
}
