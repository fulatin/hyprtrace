use crate::db::Database;
use hyprland::data::Client;
use hyprland::prelude::HyprDataActiveOptional;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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
                log::info!(
                    "Session {} marked as away (absent > {}s)",
                    id,
                    away_timeout.as_secs()
                );
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

    resolve_idle_duration(query_loginctl_idle(), activity)
}

/// Decide the idle duration from the two available sources.
///
/// logind's idle hint is only trusted when it explicitly reports
/// `IdleHint=yes`. Otherwise we fall back to the evdev-based activity
/// timestamp: on Hyprland nothing calls logind's `SetIdleHint`, so
/// `IdleHint=no` carries no information (it stays "no" even after the
/// user has been away for hours) and must not be treated as "0s idle".
fn resolve_idle_duration(
    loginctl_idle: Option<Duration>,
    activity: &ActivityState,
) -> Option<Duration> {
    if loginctl_idle.is_some() {
        return loginctl_idle;
    }

    // Last resort: Hyprland window/workspace events only.
    if let Ok(guard) = activity.last_activity.lock() {
        Some(guard.elapsed())
    } else {
        log::warn!(
            "Idle detection unavailable: loginctl reports no idle hint and \
             the activity state is inaccessible; sessions cannot be idle-detected"
        );
        None
    }
}

fn query_loginctl_idle() -> Option<Duration> {
    let output = std::process::Command::new("loginctl")
        .args([
            "show-user",
            "--property=IdleHint",
            "--property=IdleSinceHintMonotonic",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    let now_mono_us = monotonic_micros()?;
    parse_loginctl_idle(&stdout, now_mono_us)
}

/// Parse `loginctl show-user` output into an idle duration.
///
/// Returns `Some(duration)` only when logind explicitly reports
/// `IdleHint=yes` AND a parseable `IdleSinceHintMonotonic`. Anything else
/// (`IdleHint=no`, missing/garbled timestamp, empty output) maps to `None`
/// ("no idle information") so the evdev fallback stays in charge.
///
/// `IdleSinceHintMonotonic` is `CLOCK_MONOTONIC` microseconds since boot, so a
/// wall-clock jump (NTP correction, manual time change) cannot skew it, and
/// time spent in suspend is not counted — a 2h lid-close does not turn into a
/// false 2h idle span.
fn parse_loginctl_idle(stdout: &str, now_mono_us: u64) -> Option<Duration> {
    let mut idle_hint = false;
    let mut idle_since_mono: u64 = 0;

    for line in stdout.lines() {
        if line == "IdleHint=yes" {
            idle_hint = true;
        } else if let Some(val) = line.strip_prefix("IdleSinceHintMonotonic=") {
            idle_since_mono = val.parse().unwrap_or(0);
        }
    }

    if !idle_hint || idle_since_mono == 0 {
        return None;
    }

    // `checked_sub` treats a (theoretically impossible) future timestamp as
    // "no information" rather than a bogus duration.
    Some(Duration::from_micros(now_mono_us.checked_sub(idle_since_mono)?))
}

/// Current `CLOCK_MONOTONIC` time in microseconds since boot.
fn monotonic_micros() -> Option<u64> {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `ts` is a valid pointer to a `timespec` that clock_gettime fills
    // in; it returns 0 on success and -1 on error.
    let ok = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    if ok != 0 {
        return None;
    }
    Some((ts.tv_sec as u64) * 1_000_000 + (ts.tv_nsec as u64) / 1_000)
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
                        id,
                        class,
                        workspace
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

    #[test]
    fn loginctl_idle_hint_no_means_unknown_not_zero() {
        // Hyprland never calls logind's SetIdleHint, so loginctl reports
        // IdleHint=no forever. This must map to None ("no information")
        // so the evdev fallback stays in charge — NOT to Some(ZERO),
        // which would disable idle detection entirely (review H2).
        let out = "IdleHint=no\nIdleSinceHintMonotonic=0\n";
        assert!(parse_loginctl_idle(out, 1_000_000).is_none());
    }

    #[test]
    fn loginctl_idle_hint_yes_with_since_is_adopted() {
        // The idle hint was set 90s (in monotonic microseconds) before "now".
        let since_mono = 1_000_000u64;
        let now_mono = since_mono + 90_000_000;
        let out = format!("IdleHint=yes\nIdleSinceHintMonotonic={}\n", since_mono);
        let d = parse_loginctl_idle(&out, now_mono).expect("expected a duration");
        assert!((89..=91).contains(&d.as_secs()), "got {}s", d.as_secs());
    }

    #[test]
    fn loginctl_idle_hint_yes_without_usable_since_is_unknown() {
        // Missing timestamp or garbage must not be reported as 0s idle.
        assert!(parse_loginctl_idle("IdleHint=yes\n", 1_000_000).is_none());
        assert!(parse_loginctl_idle("IdleHint=yes\nIdleSinceHintMonotonic=abc\n", 1_000_000).is_none());
        assert!(parse_loginctl_idle("IdleHint=yes\nIdleSinceHintMonotonic=0\n", 1_000_000).is_none());
    }

    #[test]
    fn loginctl_empty_or_garbage_output_is_unknown() {
        assert!(parse_loginctl_idle("", 1_000_000).is_none());
        assert!(parse_loginctl_idle("garbage output", 1_000_000).is_none());
    }

    #[test]
    fn loginctl_since_in_future_is_unknown() {
        // A timestamp that is somehow ahead of "now" yields no duration
        // instead of a bogus value (handled by checked_sub).
        let out = format!("IdleHint=yes\nIdleSinceHintMonotonic={}\n", 1_000_000u64 + 60_000_000);
        assert!(parse_loginctl_idle(&out, 1_000_000).is_none());
    }

    #[test]
    fn evdev_activity_is_used_when_loginctl_has_no_hint() {
        // loginctl unavailable/uninformative -> fall back to the evdev
        // activity timestamp instead of reporting 0s idle.
        let activity = ActivityState::new();
        activity.mark_activity();
        let d = resolve_idle_duration(None, &activity).expect("evdev fallback");
        assert!(d < Duration::from_secs(5));
    }

    #[test]
    fn loginctl_result_wins_over_evdev() {
        // An explicit IdleHint=yes reading takes precedence.
        let activity = ActivityState::new();
        let d = resolve_idle_duration(Some(Duration::from_secs(1234)), &activity)
            .expect("loginctl result");
        assert_eq!(d, Duration::from_secs(1234));
    }
}
