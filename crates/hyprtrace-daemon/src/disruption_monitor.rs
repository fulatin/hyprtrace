use crate::db::Database;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Listen to D-Bus for notification events (org.freedesktop.Notifications) and
/// record them so we can quantify interruptions. Uses `gdbus monitor` as a
/// subprocess to avoid pulling in a D-Bus client library.
///
/// Also polls `wl-paste` to detect clipboard changes (copy activity).
pub struct DisruptionMonitor {
    db: Arc<Mutex<Database>>,
    running: Arc<AtomicBool>,
}

impl DisruptionMonitor {
    pub fn start(db: Arc<Mutex<Database>>) -> Self {
        let running = Arc::new(AtomicBool::new(true));
        let m = Self {
            db,
            running: running.clone(),
        };
        let db1 = m.db.clone();
        let running1 = running.clone();
        std::thread::spawn(move || {
            spawn_notification_listener(db1, running1);
        });
        let db2 = m.db.clone();
        let running2 = running.clone();
        std::thread::spawn(move || {
            spawn_clipboard_poller(db2, running2);
        });
        m
    }
}

impl Drop for DisruptionMonitor {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }
}

fn spawn_notification_listener(db: Arc<Mutex<Database>>, running: Arc<AtomicBool>) {
    // Reconnect loop: dbus-daemon can restart or dbus-monitor can exit, which
    // would otherwise leave notification tracking permanently dead until the
    // daemon restarts. Re-spawn after a short backoff while still running.
    while running.load(Ordering::Relaxed) {
        // dbus-monitor eavesdrops method calls, unlike `gdbus monitor` which
        // only surfaces signals/returns. Match only Notify method calls.
        let mut child = match Command::new("dbus-monitor")
            .args([
                "--session",
                "type=method_call,interface=org.freedesktop.Notifications,member=Notify",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                log::warn!("Notification monitor unavailable (dbus-monitor): {}", e);
                return;
            }
        };

        log::info!("Notification monitor started (D-Bus via dbus-monitor)");
        let stdout = match child.stdout.take() {
            Some(s) => s,
            None => {
                let _ = child.kill();
                return;
            }
        };
        let reader = BufReader::new(stdout);

        // dbus-monitor emits a preamble + the Notify call. We track whether
        // we're inside the argument block of a Notify call, then extract the
        // app name (1st string) and the summary (3rd string).
        //
        // Notify's signature is (app_name s, replaces_id u, app_icon s,
        // summary s, body s, ...). Only the `s` arguments are counted here, so
        // the strings are app_name / app_icon / summary / body in that order —
        // the summary is the third string (index 2), NOT the fourth.
        let mut app = String::new();
        let mut summary = String::new();
        let mut string_idx = 0usize;
        let mut in_call = false;

        for line in reader.lines() {
            if !running.load(Ordering::Relaxed) {
                break;
            }
            let line = match line {
                Ok(l) => l,
                Err(_) => break,
            };
            let line = line.trim();

            if line.starts_with("method call") && line.contains("member=Notify") {
                in_call = true;
                app.clear();
                summary.clear();
                string_idx = 0;
                continue;
            }
            if in_call {
                if let Some(s) = line.strip_prefix("string \"") {
                    // Take everything up to the LAST quote so a value that
                    // itself contains a quote (or a trailing quote) is parsed
                    // correctly instead of stripping every trailing quote.
                    let value = s
                        .rsplit_once('"')
                        .map(|(v, _)| v.to_string())
                        .unwrap_or_else(|| s.to_string());
                    match string_idx {
                        0 => app = value,
                        2 => summary = value,
                        _ => {}
                    }
                    string_idx += 1;
                } else if line.starts_with("array [")
                    || line.starts_with("dict entry")
                    || line.starts_with("}")
                {
                    // Record even when app_name is empty (many apps send an
                    // empty app_name); the `in_call` guard already ensures this
                    // is a Notify call.
                    let guard = match db.lock() {
                        Ok(g) => g,
                        Err(_) => {
                            in_call = false;
                            continue;
                        }
                    };
                    if let Err(e) = guard.save_notification(&app, &summary) {
                        log::warn!("Failed to save notification event: {}", e);
                    } else {
                        log::debug!("Notification recorded: {} - {}", app, summary);
                    }
                    in_call = false;
                }
            }
        }

        let _ = child.kill();
        // If the monitor exited while we are still supposed to be running,
        // reconnect after a short backoff instead of silently stopping.
        if running.load(Ordering::Relaxed) {
            log::warn!("Notification monitor exited; reconnecting in 5s");
            std::thread::sleep(Duration::from_secs(5));
        }
    }
    log::info!("Notification monitor stopped");
}

fn spawn_clipboard_poller(db: Arc<Mutex<Database>>, running: Arc<AtomicBool>) {
    let mut last_hash: Option<String> = None;
    loop {
        if !running.load(Ordering::Relaxed) {
            break;
        }
        std::thread::sleep(Duration::from_secs(5));

        let output = match Command::new("wl-paste").output() {
            Ok(o) if o.status.success() => o.stdout,
            _ => continue, // clipboard empty or unavailable
        };
        let hash = fnv1a_short(&output);
        if let Some(prev) = &last_hash {
            if prev != &hash {
                {
                    let guard = match db.lock() {
                        Ok(g) => g,
                        Err(_) => {
                            last_hash = Some(hash.clone());
                            continue;
                        }
                    };
                    if let Err(e) = guard.save_clipboard() {
                        log::warn!("Failed to save clipboard event: {}", e);
                    }
                }
                log::debug!("Clipboard change recorded ({} bytes)", output.len());
            }
        }
        last_hash = Some(hash);
    }
}

/// FNV-1a, truncated to the first 4 KiB. Not a cryptographic hash — it is
/// only used to tell two consecutive notification payloads apart.
fn fnv1a_short(data: &[u8]) -> String {
    // Cheap deterministic fingerprint without pulling in a hash crate.
    let mut h: u64 = 0xcbf29ce484222325;
    for b in data.iter().take(4096) {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{:x}", h)
}
