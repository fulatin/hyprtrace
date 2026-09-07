//! Send desktop notifications via D-Bus (org.freedesktop.Notifications) using
//! zbus, instead of shelling out to `notify-send`. This keeps notifications
//! working even when the `notify-send` binary is absent, and centralises the
//! app name / timeout used by every caller.

use std::collections::HashMap;
use zbus::blocking::Connection;
use zbus::zvariant::Value;

const APP_NAME: &str = "hyprtrace";
const TIMEOUT_MS: i32 = 5000;

/// Send a notification. Best-effort: falls back to `notify-send` if the D-Bus
/// connection is unavailable (e.g. a session without a notification daemon).
pub fn notify(summary: &str, body: &str) {
    log::info!("Notification: {} - {}", summary, body);
    match notify_via_zbus(summary, body) {
        Ok(()) => {}
        Err(e) => {
            log::warn!("zbus notify failed ({}); falling back to notify-send", e);
            let _ = std::process::Command::new("notify-send")
                .args(["-a", APP_NAME, summary, body])
                .spawn();
        }
    }
}

fn notify_via_zbus(summary: &str, body: &str) -> zbus::Result<()> {
    let conn = Connection::session()?;
    let proxy = zbus::blocking::Proxy::new(
        &conn,
        "org.freedesktop.Notifications",
        "/org/freedesktop/Notifications",
        "org.freedesktop.Notifications",
    )?;

    // Notify(app_name s, replaces_id u, app_icon s, summary s, body s,
    //        actions as, hints a{sv}, expire_timeout i) -> u32 (notification id)
    let _: u32 = proxy.call(
        "Notify",
        &(
            APP_NAME.to_string(),
            0u32,
            String::new(),
            summary.to_string(),
            body.to_string(),
            Vec::<String>::new(),
            HashMap::<String, Value>::new(),
            TIMEOUT_MS,
        ),
    )?;
    Ok(())
}
