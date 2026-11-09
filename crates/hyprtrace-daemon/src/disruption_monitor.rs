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
        let mut parser = NotifyParser::default();

        for line in reader.lines() {
            if !running.load(Ordering::Relaxed) {
                break;
            }
            let line = match line {
                Ok(l) => l,
                Err(_) => break,
            };
            let trimmed = line.trim();
            if trimmed.starts_with("method call") && trimmed.contains("member=Notify") {
                parser.begin_call();
                continue;
            }
            let Some(notification) = parser.feed(&line) else {
                continue;
            };
            let guard = match db.lock() {
                Ok(g) => g,
                Err(_) => continue,
            };
            if let Err(e) = guard.save_notification(&notification.app, &notification.summary) {
                log::warn!("Failed to save notification event: {}", e);
            } else {
                log::debug!(
                    "Notification recorded: {} - {}",
                    notification.app,
                    notification.summary
                );
            }
        }

        let _ = child.kill();
        // Reap the child: without `wait()` every reconnect leaks a zombie
        // process (the old loop could respawn every 5 s on a dead session bus).
        let _ = child.wait();
        // If the monitor exited while we are still supposed to be running,
        // reconnect after a short backoff instead of silently stopping.
        if running.load(Ordering::Relaxed) {
            log::warn!("Notification monitor exited; reconnecting in 5s");
            std::thread::sleep(Duration::from_secs(5));
        }
    }
    log::info!("Notification monitor stopped");
}

/// One notification extracted from the `dbus-monitor` stream.
#[derive(Debug, Default, PartialEq, Eq)]
struct Notification {
    app: String,
    summary: String,
}

/// Line-oriented parser for the `dbus-monitor` output of
/// `org.freedesktop.Notifications.Notify` method calls.
///
/// `dbus-monitor` prints D-Bus string arguments verbatim (no escaping), so a
/// value that contains a quote or a newline spans several output lines. The old
/// reader (`trim_end_matches('"')` per line) corrupted those values: a quote
/// cut the value short and a newline silently truncated it (issue #28). This
/// state machine accumulates continuation lines and only needs the `s`
/// arguments, which are counted separately from the numeric ones.
///
/// Notify's signature is (app_name s, replaces_id u, app_icon s, summary s,
/// body s, ...), so app_name is string 0 and summary is string 2.
#[derive(Default)]
struct NotifyParser {
    in_call: bool,
    /// Index into the call's `s` arguments (numeric args are not counted).
    string_index: usize,
    notification: Notification,
    /// Value of a string whose closing quote has not been seen yet — i.e. a
    /// value containing a raw newline, which spans output lines.
    pending: Option<String>,
}

impl NotifyParser {
    /// Start a new Notify call, discarding any partial state.
    fn begin_call(&mut self) {
        self.in_call = true;
        self.string_index = 0;
        self.notification = Notification::default();
        self.pending = None;
    }

    fn store_string(&mut self, value: String) {
        match self.string_index {
            0 => self.notification.app = value,
            2 => self.notification.summary = value,
            _ => {}
        }
        self.string_index += 1;
    }

    /// Feed one output line. Returns the notification once the call's argument
    /// block ends.
    fn feed(&mut self, line: &str) -> Option<Notification> {
        if !self.in_call {
            return None;
        }
        let line = line.strip_suffix('\r').unwrap_or(line);

        // Continuation of a string whose value contains a newline. The value is
        // printed verbatim, so this line is data, not a new argument.
        if let Some(mut value) = self.pending.take() {
            value.push('\n');
            if let Some(rest) = line.strip_suffix('"') {
                value.push_str(rest);
                self.store_string(value);
            } else {
                value.push_str(line);
                self.pending = Some(value);
            }
            return None;
        }

        // Argument lines carry a leading indent; continuation lines do not, so
        // only token detection trims.
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("string \"") {
            match rest.strip_suffix('"') {
                Some(value) => self.store_string(value.to_string()),
                None => self.pending = Some(rest.to_string()),
            }
            return None;
        }

        // The argument block ends with the hints dictionary / array.
        if trimmed.starts_with("array [")
            || trimmed.starts_with("dict entry")
            || trimmed.starts_with('}')
        {
            self.in_call = false;
            return Some(std::mem::take(&mut self.notification));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(lines: &[&str]) -> Vec<Notification> {
        let mut parser = NotifyParser::default();
        parser.begin_call();
        lines.iter().filter_map(|line| parser.feed(line)).collect()
    }

    #[test]
    fn extracts_app_name_and_summary_only() {
        let out = parse(&[
            "   string \"firefox\"",
            "   uint32 0",
            "   string \"firefox\"",
            "   string \"Build finished\"",
            "   string \"main.rs compiled\"",
            "   array [",
            "   ]",
            "   int32 5000",
        ]);
        assert_eq!(
            out,
            vec![Notification {
                app: "firefox".to_string(),
                summary: "Build finished".to_string(),
            }]
        );
    }

    #[test]
    fn an_empty_app_name_is_still_recorded() {
        let out = parse(&[
            "   string \"\"",
            "   uint32 0",
            "   string \"\"",
            "   string \"Download complete\"",
            "   array [",
        ]);
        assert_eq!(
            out,
            vec![Notification {
                app: String::new(),
                summary: "Download complete".to_string(),
            }]
        );
    }

    #[test]
    fn quotes_inside_a_value_are_kept() {
        // `say "hi"` is printed as      string "say "hi""
        // → only the final quote closes the argument.
        let out = parse(&[
            "   string \"app\"",
            "   uint32 0",
            "   string \"icon\"",
            "   string \"say \"hi\"\"",
            "   string \"\"",
            "   array [",
        ]);
        assert_eq!(out[0].summary, "say \"hi\"");
    }

    #[test]
    fn newlines_inside_a_value_are_kept() {
        // A summary that contains a newline spans output lines; the old
        // per-line reader stored only "first line" (issue #28).
        let out = parse(&[
            "   string \"app\"",
            "   uint32 0",
            "   string \"icon\"",
            "   string \"first line",
            "second line\"",
            "   string \"body\"",
            "   array [",
        ]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].summary, "first line\nsecond line");
    }

    #[test]
    fn a_new_call_resets_the_previous_state() {
        let mut parser = NotifyParser::default();
        parser.begin_call();
        parser.feed("   string \"old\"");
        parser.begin_call();
        let out: Vec<Notification> = [
            "   string \"new\"",
            "   uint32 1",
            "   string \"icon\"",
            "   string \"fresh\"",
            "   array [",
        ]
        .iter()
        .filter_map(|line| parser.feed(line))
        .collect();
        assert_eq!(
            out,
            vec![Notification {
                app: "new".to_string(),
                summary: "fresh".to_string(),
            }]
        );
    }
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
