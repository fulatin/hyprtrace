use crate::config::Config;
use crate::db::Database;
use anyhow::Context;
use hyprland::event_listener::EventListener;
use std::sync::{Arc, Mutex};

pub struct WindowTracker {
    db: Arc<Mutex<Database>>,
    config: Config,
}

impl WindowTracker {
    pub fn new(db: Arc<Mutex<Database>>, config: Config) -> Self {
        Self { db, config }
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        // Delete orphaned sessions from previous crashes/improper shutdowns.
        // end_session() would compute duration_ms from original start to now,
        // producing absurdly long sessions (e.g. hours of phantom time) and
        // corrupting daily_summary. Deleting them is safer.
        if let Ok(guard) = self.db.lock() {
            let count = guard.clear_orphaned_sessions().unwrap_or(0);
            if count > 0 {
                log::info!("Cleared {} orphaned session(s) from previous run", count);
            }
        }

        let db = self.db.clone();
        let mut listener = EventListener::new();

        listener.add_active_window_changed_handler(move |data| {
            let db = match db.lock() {
                Ok(guard) => guard,
                Err(e) => {
                    log::error!("Failed to acquire DB lock: {}", e);
                    return;
                }
            };

            match data {
                Some(win_data) => {
                    let class = win_data.class.to_lowercase();
                    let title = &win_data.title;

                    if let Some(prev_id) = end_current_session(&db) {
                        log::info!("Ended session {}", prev_id);
                    }

                    match db.start_session(&class, title, "") {
                        Ok(new_id) => {
                            log::info!("Started session {}: class={}, title={}", new_id, class, title);
                        }
                        Err(e) => log::error!("Failed to start session: {}", e),
                    }
                }
                None => {
                    if let Some(prev_id) = end_current_session(&db) {
                        log::info!("Switched to idle, ended session {}", prev_id);
                    }
                    log::debug!("Entered idle state");
                }
            }
        });

        log::info!(
            "HyprTrace daemon started, listening for window switch events (idle_timeout={}s)...",
            self.config.daemon.idle_timeout_seconds
        );
        listener
            .start_listener()
            .context("Failed to start Hyprland event listener! Make sure Hyprland is running.")
    }
}

fn end_current_session(db: &Database) -> Option<i64> {
    db.end_current_session().ok().flatten()
}
