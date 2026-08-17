use crate::db::Database;
use hyprland::data::Client;
use hyprland::prelude::HyprDataActiveOptional;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone)]
pub struct ActivityState {
    /// Last activity timestamp driven by Hyprland events (window switches,
    /// workspace/monitor changes). Used as a fallback idle source.
    pub last_activity: Arc<Mutex<Instant>>,
    /// Last timestamp of a physical keyboard/mouse event from the evdev input
    /// monitor. This is the authoritative "no mouse movement and no key
    /// presses" signal when the input monitor is running.
    pub last_input: Arc<Mutex<Instant>>,
    pub is_idle: Arc<AtomicBool>,
    input_monitor_active: Arc<AtomicBool>,
}

impl ActivityState {
    pub fn new() -> Self {
        Self {
            last_activity: Arc::new(Mutex::new(Instant::now())),
            last_input: Arc::new(Mutex::new(Instant::now())),
            is_idle: Arc::new(AtomicBool::new(false)),
            input_monitor_active: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn mark_activity(&self) {
        if let Ok(mut guard) = self.last_activity.lock() {
            *guard = Instant::now();
        }
        self.is_idle.store(false, Ordering::Release);
    }

    /// Record a physical keyboard/mouse input event.
    pub fn mark_input_activity(&self) {
        if let Ok(mut guard) = self.last_input.lock() {
            *guard = Instant::now();
        }
        self.mark_activity();
    }

    pub fn mark_input_monitor_active(&self) {
        self.input_monitor_active.store(true, Ordering::Release);
    }

    pub fn is_input_monitor_active(&self) -> bool {
        self.input_monitor_active.load(Ordering::Acquire)
    }
}

pub fn spawn_idle_monitor(
    db: Arc<Mutex<Database>>,
    activity: ActivityState,
    idle_timeout_seconds: u64,
    focused_threshold_seconds: u64,
) {
    let away_timeout_seconds = idle_timeout_seconds * 2;
    let timeout = Duration::from_secs(idle_timeout_seconds);
    let away_timeout = Duration::from_secs(away_timeout_seconds);
    let poll_interval = Duration::from_secs((idle_timeout_seconds / 4).max(5).min(30));

    std::thread::spawn(move || {
        log::info!(
            "Idle monitor started (timeout={}s, away={}s, focused={}s, poll={}s)",
            timeout.as_secs(),
            away_timeout.as_secs(),
            focused_threshold_seconds,
            poll_interval.as_secs(),
        );

        // Tracks the session that was ended due to idle, so it can be
        // retroactively marked "away" if the absence continues.
        let mut last_idle_ended: Option<i64> = None;

        loop {
            std::thread::sleep(poll_interval);

            if let Err(e) = tick(
                &db,
                &activity,
                timeout,
                away_timeout,
                focused_threshold_seconds,
                &mut last_idle_ended,
            ) {
                log::error!("Idle monitor tick error: {}", e);
            }
        }
    });
}

fn tick(
    db: &Arc<Mutex<Database>>,
    activity: &ActivityState,
    timeout: Duration,
    away_timeout: Duration,
    focused_threshold_seconds: u64,
    last_idle_ended: &mut Option<i64>,
) -> anyhow::Result<()> {
    let idle_duration = match get_idle_duration(activity) {
        Some(d) => d,
        None => return Ok(()),
    };

    if activity.is_idle.load(Ordering::Acquire) {
        if idle_duration < timeout {
            log::info!("User activity resumed");
            activity.is_idle.store(false, Ordering::Release);
            *last_idle_ended = None;
            resume_session(db)?;
        } else if idle_duration >= away_timeout {
            // Session already ended at the idle timeout; if the user is STILL
            // absent past the away threshold, retroactively mark it "away".
            if let Some(id) = last_idle_ended.take() {
                let guard = db.lock().map_err(|e| anyhow::anyhow!("DB lock: {}", e))?;
                guard.set_session_state_only(id, "away")?;
                log::info!("Session {} marked as away (absent > {}s)", id, away_timeout.as_secs());
            }
        }
        return Ok(());
    }

    let session = {
        let guard = db.lock().map_err(|e| anyhow::anyhow!("DB lock: {}", e))?;
        guard.current_session_state()?
    };

    let (session_id, state, _focused_ms) = match session {
        Some(s) => s,
        None => return Ok(()),
    };

    if idle_duration >= timeout {
        log::info!(
            "Idle for {:.0}s (timeout: {}s), marking idle",
            idle_duration.as_secs_f64(),
            timeout.as_secs()
        );

        let guard = db.lock().map_err(|e| anyhow::anyhow!("DB lock: {}", e))?;
        guard.update_session_state(session_id, "idle")?;
        guard.update_ongoing_focused_ms(session_id)?;
        guard.end_session(session_id)?;
        activity.is_idle.store(true, Ordering::Release);
        *last_idle_ended = Some(session_id);
        log::info!("Ended session {} due to idle timeout", session_id);
        return Ok(());
    }

    let session_duration = {
        let guard = db.lock().map_err(|e| anyhow::anyhow!("DB lock: {}", e))?;
        // Monotonic elapsed time: immune to wall-clock jumps (dual-boot RTC
        // skew) and never negative.
        (guard.session_elapsed_ms(session_id)? / 1000) as u64
    };

    if state != "focused" && session_duration >= focused_threshold_seconds {
        log::info!(
            "Same window for {}s (threshold: {}s), entering focused state",
            session_duration,
            focused_threshold_seconds
        );
        let guard = db.lock().map_err(|e| anyhow::anyhow!("DB lock: {}", e))?;
        guard.update_session_state(session_id, "focused")?;
    }

    if state == "focused" {
        let guard = db.lock().map_err(|e| anyhow::anyhow!("DB lock: {}", e))?;
        guard.update_ongoing_focused_ms(session_id)?;
    }

    Ok(())
}

fn get_idle_duration(activity: &ActivityState) -> Option<Duration> {
    // When the evdev input monitor is running, idle means "no physical mouse
    // movement and no keyboard events for >= idle_timeout". This is the most
    // direct signal and is more reliable than loginctl on Hyprland sessions.
    if activity.is_input_monitor_active() {
        if let Ok(guard) = activity.last_input.lock() {
            return Some(guard.elapsed());
        }
    }

    // Fallback: loginctl session idle hint (works on many systemd setups).
    if let Some(dur) = query_loginctl_idle() {
        return Some(dur);
    }

    // Last resort: Hyprland window/workspace events only.
    if let Ok(guard) = activity.last_activity.lock() {
        Some(guard.elapsed())
    } else {
        None
    }
}

fn query_loginctl_idle() -> Option<Duration> {
    let output = std::process::Command::new("loginctl")
        .args(["show-user", "--property=IdleHint", "--property=IdleSinceHint"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    let mut idle_hint = false;
    let mut idle_since: u64 = 0;

    for line in stdout.lines() {
        if line == "IdleHint=yes" {
            idle_hint = true;
        } else if let Some(val) = line.strip_prefix("IdleSinceHint=") {
            idle_since = val.parse().ok()?;
        }
    }

    if !idle_hint || idle_since == 0 {
        return Some(Duration::ZERO);
    }

    let since = UNIX_EPOCH + Duration::from_micros(idle_since);
    SystemTime::now()
        .duration_since(since)
        .ok()
        .or(Some(Duration::ZERO))
}

fn resume_session(db: &Arc<Mutex<Database>>) -> anyhow::Result<()> {
    match Client::get_active() {
        Ok(Some(client)) => {
            let class = client.class.to_lowercase();
            let title = client.title;
            let workspace = client.workspace.name;

            let guard = db.lock().map_err(|e| anyhow::anyhow!("DB lock: {}", e))?;
            match guard.start_session(&class, &title, &workspace, Some(client.pid)) {
                Ok(id) => {
                    log::info!(
                        "Resumed session {}: class={}, workspace={}",
                        id, class, workspace
                    );
                }
                Err(e) => log::error!("Failed to start session on resume: {}", e),
            }
        }
        Ok(None) => {
            log::debug!("Activity resumed but no active window");
        }
        Err(e) => {
            log::error!("Failed to query active window on resume: {}", e);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_monitor_active_uses_physical_input_time() {
        let a = ActivityState::new();
        a.mark_input_monitor_active();

        // Physical keyboard/mouse last seen 10s ago, but a Hyprland event
        // (e.g. a focus change) happened just now. The idle monitor must use
        // the physical input timestamp while the input monitor is running.
        *a.last_input.lock().unwrap() = Instant::now() - Duration::from_secs(10);
        *a.last_activity.lock().unwrap() = Instant::now();

        let d = get_idle_duration(&a).unwrap();
        assert!(d.as_secs() >= 9 && d.as_secs() <= 11);
    }

    #[test]
    fn mark_input_activity_updates_both_timestamps() {
        let a = ActivityState::new();
        let before = Instant::now();
        a.mark_input_activity();
        assert!(*a.last_input.lock().unwrap() >= before);
        assert!(*a.last_activity.lock().unwrap() >= before);
    }
}
