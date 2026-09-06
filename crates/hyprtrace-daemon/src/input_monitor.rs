use crate::idle_monitor::ActivityState;
use evdev::{EventType, KeyCode};
use std::path::PathBuf;
use std::time::Duration;

/// Spawn a thread that watches Linux input devices (keyboard/mouse/touchpad)
/// and marks the user as active on any event.
///
/// This complements the `loginctl` idle check: on systems where loginctl is
/// unavailable, the fallback previously only noticed window switches, so a
/// user typing or moving the mouse inside a single window could be falsely
/// marked idle. Requires read access to `/dev/input/event*` (the `input`
/// group); when permission is missing the monitor degrades gracefully and
/// logs a hint.
pub fn spawn_input_monitor(activity: ActivityState) {
    std::thread::spawn(move || {
        let devices = scan_input_devices();
        if devices.is_empty() {
            // Distinguish "no devices" from "no permission": if /dev/input has
            // event nodes but we could not open any, it is almost certainly a
            // permissions problem.
            let has_event_nodes = std::fs::read_dir("/dev/input")
                .map(|rd| {
                    rd.flatten()
                        .any(|e| e.file_name().to_string_lossy().starts_with("event"))
                })
                .unwrap_or(false);
            if has_event_nodes {
                log::warn!(
                    "Input monitor disabled: found /dev/input/event* but could not open them — \
                     add your user to the input group for keyboard/mouse activity detection: \
                     sudo usermod -aG input $USER (then re-login)"
                );
            } else {
                log::warn!("Input monitor: no /dev/input/event* devices found");
            }
            return;
        }

        let mut devices: Vec<evdev::Device> = devices.into_iter().map(|(_, d)| d).collect();
        log::info!("Input monitor started ({} device(s))", devices.len());
        activity.mark_input_monitor_active();

        loop {
            for dev in &mut devices {
                if let Ok(events) = dev.fetch_events() {
                    for ev in events {
                        // EV_KEY press/repeat (value 1/2) or any REL/ABS
                        // movement: value != 0 means real input, ignore
                        // sync/release noise.
                        if ev.value() != 0 {
                            activity.mark_input_activity();
                        }
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    });
}

/// Key codes that do not represent genuine user input. A power button, lid
/// switch or suspend key firing is not "the user typed or moved the mouse",
/// and would otherwise pollute the idle detector.
fn is_ignored_key(code: KeyCode) -> bool {
    matches!(
        code,
        KeyCode::KEY_POWER
            | KeyCode::KEY_POWER2
            | KeyCode::KEY_SLEEP
            | KeyCode::KEY_SUSPEND
            | KeyCode::KEY_WAKEUP
    )
}

/// Discover input devices that can report genuine user input.
///
/// Uses `evdev::enumerate()` — which scans `/dev/input/event*`, opens each
/// device and reads its capabilities — instead of a hand-written `read_dir`
/// filtered by the `event` filename prefix. It then filters out power/switch
/// devices (e.g. a `Lid Switch` or `Power Button`) that fire `KEY_POWER` /
/// `KEY_SLEEP` etc. but are not keyboards, mice or touchpads.
fn scan_input_devices() -> Vec<(PathBuf, evdev::Device)> {
    let mut out = Vec::new();
    for (path, dev) in evdev::enumerate() {
        // Must actually report key events.
        if !dev.supported_events().contains(EventType::KEY) {
            continue;
        }
        // Must have at least one "real" key, not only power/lid/suspend.
        let has_real_key = dev
            .supported_keys()
            .map(|keys| keys.iter().any(|k| !is_ignored_key(k)))
            .unwrap_or(false);
        if !has_real_key {
            continue;
        }
        if dev.set_nonblocking(true).is_err() {
            log::warn!(
                "Input monitor: failed to set nonblocking on {}",
                path.display()
            );
            continue;
        }
        out.push((path, dev));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_and_switch_keys_are_ignored() {
        for k in [
            KeyCode::KEY_POWER,
            KeyCode::KEY_POWER2,
            KeyCode::KEY_SLEEP,
            KeyCode::KEY_SUSPEND,
            KeyCode::KEY_WAKEUP,
        ] {
            assert!(is_ignored_key(k), "{k:?} should be ignored");
        }
    }

    #[test]
    fn normal_keys_are_not_ignored() {
        for k in [KeyCode::KEY_A, KeyCode::KEY_ENTER, KeyCode::BTN_LEFT] {
            assert!(!is_ignored_key(k), "{k:?} must count as real input");
        }
    }
}
