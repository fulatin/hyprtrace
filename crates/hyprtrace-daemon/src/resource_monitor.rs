use crate::db::Database;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Sample CPU/memory of the currently focused window's process every interval
/// and store the readings in `app_resources`. CPU% is computed from the delta
/// of /proc/<pid>/stat jiffies between samples so it reflects utilization, not
/// an instantaneous snapshot.
pub fn spawn_resource_monitor(db: Arc<Mutex<Database>>, interval_secs: u64) {
    std::thread::spawn(move || {
        let interval = Duration::from_secs(interval_secs.max(5));
        let ncpu = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
            .max(1) as f64;

        // (pid, utime+stime jiffies, wall time of that reading)
        let mut last: Option<(i32, u64, std::time::Instant)> = None;

        loop {
            std::thread::sleep(interval);

            let current = {
                let guard = match db.lock() {
                    Ok(g) => g,
                    Err(e) => {
                        log::warn!("Resource monitor: DB lock failed: {}", e);
                        last = None;
                        continue;
                    }
                };
                match guard.current_session_resources() {
                    Ok(Some(x)) => x,
                    _ => {
                        last = None;
                        continue;
                    }
                }
            };
            let (session_id, class, pid) = current;

            let jiffies = match read_proc_jiffies(pid) {
                Some(j) => j,
                None => {
                    last = None;
                    continue;
                }
            };
            let mem_kb = read_proc_mem_kb(pid).unwrap_or(0);
            let now = std::time::Instant::now();

            let cpu_pct = match &last {
                Some((lp, lj, lt)) if *lp == pid => {
                    let dt = now.duration_since(*lt).as_secs_f64();
                    if dt > 0.0 {
                        let dj = jiffies.saturating_sub(*lj) as f64;
                        (dj / dt / ncpu * 100.0).clamp(0.0, 100.0 * ncpu)
                    } else {
                        0.0
                    }
                }
                _ => 0.0,
            };

            {
                let guard = match db.lock() {
                    Ok(g) => g,
                    Err(e) => {
                        log::warn!("Resource monitor: DB lock failed: {}", e);
                        last = None;
                        continue;
                    }
                };
                if let Err(e) = guard.save_resource_sample(session_id, &class, cpu_pct, mem_kb) {
                    log::warn!("Resource monitor: failed to save sample: {}", e);
                }
            }
            log::trace!(
                "Resource sample: session={} class={} pid={} cpu={:.1}% mem={}KB",
                session_id, class, pid, cpu_pct, mem_kb
            );

            last = Some((pid, jiffies, now));
        }
    });
}

/// /proc/<pid>/stat fields (1-indexed): 14=utime, 15=stime (clock ticks).
fn read_proc_jiffies(pid: i32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{}/stat", pid)).ok()?;
    // The comm field (2) can contain spaces/parens; find the last ')' then
    // skip the following space. Fields after that are reliably indexed.
    let close = stat.rfind(')')?;
    let rest = &stat[close + 1..];
    let fields: Vec<&str> = rest.split_whitespace().collect();
    // rest starts right after ')', so index 0 here == field 3 in /proc format.
    let utime: u64 = fields.get(11)?.parse().ok()?; // field 14
    let stime: u64 = fields.get(12)?.parse().ok()?; // field 15
    Some(utime + stime)
}

/// RSS in KB from /proc/<pid>/status VmRSS.
fn read_proc_mem_kb(pid: i32) -> Option<i64> {
    let status = std::fs::read_to_string(format!("/proc/{}/status", pid)).ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            let kb: i64 = rest.trim().trim_end_matches("kB").trim().parse().ok()?;
            return Some(kb);
        }
    }
    None
}
