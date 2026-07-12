mod config;
mod db;
mod listener;

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
    let db = db::Database::open(&db_path).context("Failed to open database")?;
    db.migrate().context("Database migration failed")?;
    log::info!("Database ready: {:?}", db_path);

    let db = Arc::new(Mutex::new(db));

    let (tx, rx) = std::sync::mpsc::channel();
    let db_shutdown = db.clone();
    ctrlc::set_handler(move || {
        log::info!("Received termination signal, shutting down gracefully...");
        if let Ok(guard) = db_shutdown.lock() {
            if let Some(id) = guard.current_session_id().ok().flatten() {
                if let Err(e) = guard.end_session(id) {
                    log::error!("Failed to end active session {}: {}", id, e);
                } else {
                    log::info!("Ended active session {} on shutdown", id);
                }
            }
        }
        tx.send(()).ok();
    })?;

    let mut tracker = listener::WindowTracker::new(db, cfg);
    std::thread::spawn(move || {
        if let Err(e) = tracker.run() {
            log::error!("Window tracker exited with error: {}", e);
        }
    });

    rx.recv().ok();
    log::info!("HyprTrace daemon exited");
    Ok(())
}
