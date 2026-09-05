use crate::idle_monitor::ActivityState;
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
        let paths = scan_input_devices();
        if paths.is_empty() {
            log::warn!("Input monitor: no /dev/input/event* devices found");
            return;
        }

        let mut devices: Vec<evdev::Device> = Vec::new();
        let mut first_failure: Option<std::io::Error> = None;
        for path in &paths {
            match evdev::Device::open(path) {
                Ok(dev) => {
                    if dev.set_nonblocking(true).is_err() {
                        log::warn!(
                            "Input monitor: failed to set nonblocking on {}",
                            path.display()
                        );
                        continue;
                    }
                    devices.push(dev);
                }
                Err(e) => {
                    if first_failure.is_none() {
                        first_failure = Some(e);
                    }
                }
            }
        }

        if devices.is_empty() {
            log::warn!(
                "Input monitor disabled: cannot open /dev/input/event* ({}) — \
                 add your user to the input group for keyboard/mouse activity \
                 detection: sudo usermod -aG input $USER (then re-login)",
                first_failure
                    .map(|e| e.to_string())
                    .unwrap_or_else(|| "unknown error".to_string())
            );
            return;
        }
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

fn scan_input_devices() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir("/dev/input") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("event") {
                out.push(entry.path());
            }
        }
    }
    out.sort();
    out
}
