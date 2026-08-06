use crate::config::Config;
use crate::db::Database;
use crate::idle_monitor::ActivityState;
use anyhow::Context;
use hyprland::event_listener::EventListener;
use hyprland::data::Client;
use hyprland::prelude::HyprDataActiveOptional;
use std::sync::{Arc, Mutex};

pub struct WindowTracker {
    db: Arc<Mutex<Database>>,
    config: Config,
    activity: ActivityState,
}

impl WindowTracker {
    pub fn new(db: Arc<Mutex<Database>>, config: Config, activity: ActivityState) -> Self {
        Self { db, config, activity }
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        if let Ok(guard) = self.db.lock() {
            let count = guard.clear_orphaned_sessions().unwrap_or(0);
            if count > 0 {
                log::info!("Cleared {} orphaned session(s) from previous run", count);
            }
        }

        let db = self.db.clone();
        let activity = self.activity.clone();
        let activity2 = activity.clone();
        let activity3 = activity.clone();
        let mut listener = EventListener::new();

        listener.add_active_window_changed_handler(move |data| {
            activity.mark_activity();

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

                    let workspace = Client::get_active()
                        .ok()
                        .flatten()
                        .map(|c| c.workspace.name)
                        .unwrap_or_default();

                    if let Some(prev_id) = end_current_session(&db) {
                        log::info!("Ended session {}", prev_id);
                    }

                    match db.start_session(&class, title, &workspace) {
                        Ok(new_id) => {
                            log::info!(
                                "Started session {}: class={}, workspace={}",
                                new_id, class, workspace
                            );
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

        let activity2 = activity2;
        listener.add_workspace_changed_handler(move |_data| {
            activity2.mark_activity();
            log::trace!("Activity marked (workspace changed)");
        });

        let activity3 = activity3;
        listener.add_active_monitor_changed_handler(move |_data| {
            activity3.mark_activity();
            log::trace!("Activity marked (monitor changed)");
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
