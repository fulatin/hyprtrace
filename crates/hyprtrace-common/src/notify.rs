//! Desktop notifications over D-Bus (`org.freedesktop.Notifications`).
//!
//! Shared by the daemon and the server so there is one implementation of the
//! app name, timeout and fallback. Using zbus instead of shelling out to
//! `notify-send` keeps notifications working on systems where that binary is
//! not installed (issue #28) — minimal installs and non-systemd sessions used
//! to lose notifications silently.

use std::collections::HashMap;
use zbus::blocking::Connection;
use zbus::zvariant::Value;

const APP_NAME: &str = "hyprtrace";
const TIMEOUT_MS: i32 = 5000;

/// How prominently the notification daemon should present the message.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Urgency {
    Normal,
    Critical,
}

/// Send a normal-priority desktop notification.
///
/// Best-effort and blocking (D-Bus round-trip is short). Async callers should
/// run it on `tokio::task::spawn_blocking`. When the D-Bus call fails — e.g. a
/// session without a notification daemon — it falls back to `notify-send` if
/// that binary exists; failure of both is only logged, never propagated.
pub fn send(summary: &str, body: &str) {
    send_with_urgency(summary, body, Urgency::Normal);
}

/// Send a critical notification (the daemon's late-night wellbeing reminder
/// used `notify-send -u critical` before the zbus switch; zbus sends the
/// `urgency` hint instead).
pub fn send_critical(summary: &str, body: &str) {
    send_with_urgency(summary, body, Urgency::Critical);
}

fn send_with_urgency(summary: &str, body: &str, urgency: Urgency) {
    log::info!("Notification: {} - {}", summary, body);
    match send_via_zbus(summary, body, urgency) {
        Ok(()) => {}
        Err(e) => {
            log::warn!("zbus notify failed ({}); falling back to notify-send", e);
            let mut command = std::process::Command::new("notify-send");
            command.args(["-a", APP_NAME]);
            if urgency == Urgency::Critical {
                command.args(["-u", "critical"]);
            }
            let _ = command.args([summary, body]).spawn();
        }
    }
}

fn send_via_zbus(summary: &str, body: &str, urgency: Urgency) -> zbus::Result<()> {
    let conn = Connection::session()?;
    let proxy = zbus::blocking::Proxy::new(
        &conn,
        "org.freedesktop.Notifications",
        "/org/freedesktop/Notifications",
        "org.freedesktop.Notifications",
    )?;

    // Notify(app_name s, replaces_id u, app_icon s, summary s, body s,
    //        actions as, hints a{sv}, expire_timeout i) -> u32 (notification id)
    let mut hints: HashMap<String, Value> = HashMap::new();
    if urgency == Urgency::Critical {
        // org.freedesktop.Notifications: urgency BYTE, 1 = normal, 2 = critical.
        hints.insert("urgency".to_string(), Value::U8(2));
    }

    let _: u32 = proxy.call(
        "Notify",
        &(
            APP_NAME.to_string(),
            0u32,
            String::new(),
            summary.to_string(),
            body.to_string(),
            Vec::<String>::new(),
            hints,
            TIMEOUT_MS,
        ),
    )?;
    Ok(())
}
