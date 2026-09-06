use crate::db::Database;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Sample CPU/memory of the currently focused window's process every interval
/// and store the readings in `app_resources`. CPU% is computed from the delta
/// of /proc/<pid>/stat jiffies between samples (converted to seconds via
/// CLK_TCK) so it reflects utilization, not an instantaneous snapshot.
pub fn spawn_resource_monitor(db: Arc<Mutex<Database>>, interval_secs: u64) {
    std::thread::spawn(move || {
        let interval = Duration::from_secs(interval_secs.max(5));
        let ncpu = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
            .max(1) as f64;
        let clk_tck = clock_ticks_per_second();

        // (pid, utime+stime jiffies, wall time of that reading)
        let mut last: Option<(i32, u64, u64, std::time::Instant)> = None;

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

            // Sample CPU ticks + process start time. The start time lets us
            // detect a reused PID (the old process exited and a new one took
            // its id), so we don't attribute the new process's CPU time to the
            // old sample.
            let (jiffies, starttime) = match read_proc_sample(pid) {
                Some(x) => x,
                None => {
                    last = None;
                    continue;
                }
            };
            // A process without an RSS line (e.g. a kernel thread) reports
            // None; skip it rather than recording a bogus 0 MB.
            let mem_kb = match read_proc_mem_kb(pid) {
                Some(kb) => kb,
                None => {
                    last = None;
                    continue;
                }
            };
            let now = std::time::Instant::now();

            let cpu_pct = match &last {
                Some((lp, lstart, lj, lt))
                    if *lp == pid && *lstart == starttime =>
                {
                    let dj = jiffies.saturating_sub(*lj);
                    let dt = now.duration_since(*lt).as_secs_f64();
                    compute_cpu_pct(dj, dt, ncpu, clk_tck)
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
                session_id,
                class,
                pid,
                cpu_pct,
                mem_kb
            );

            last = Some((pid, starttime, jiffies, now));
        }
    });
}

/// Clock ticks per second for /proc/<pid>/stat jiffies. Linux exports this
/// via sysconf(_SC_CLK_TCK); it is 100 on every mainstream kernel, which we
/// use as a fallback if sysconf fails.
fn clock_ticks_per_second() -> f64 {
    let tck = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if tck > 0 {
        tck as f64
    } else {
        100.0
    }
}

/// Convert a jiffies delta into a CPU percentage normalized over all cores.
///
/// `dj` is the utime+stime delta in clock ticks; ticks must be divided by
/// CLK_TCK to obtain CPU-seconds before dividing by wall time (see review
/// H3: the missing conversion inflated readings ~CLK_TCK-fold, mostly
/// clamped to the 100*ncpu ceiling).
fn compute_cpu_pct(dj: u64, dt: f64, ncpu: f64, clk_tck: f64) -> f64 {
    if dt <= 0.0 || clk_tck <= 0.0 || ncpu <= 0.0 {
        return 0.0;
    }
    let cpu_secs = dj as f64 / clk_tck;
    (cpu_secs / dt / ncpu * 100.0).clamp(0.0, 100.0 * ncpu)
}

/// /proc/<pid>/stat fields (1-indexed): 14=utime, 15=stime (clock ticks),
/// 22=starttime (process start in clock ticks since boot, used to detect PID
/// reuse).
fn read_proc_sample(pid: i32) -> Option<(u64, u64)> {
    let stat = std::fs::read_to_string(format!("/proc/{}/stat", pid)).ok()?;
    parse_proc_stat(&stat)
}

fn parse_proc_stat(stat: &str) -> Option<(u64, u64)> {
    // The comm field (2) can contain spaces/parens; find the last ')' then
    // skip the following space. Fields after that are reliably indexed.
    let close = stat.rfind(')')?;
    let rest = &stat[close + 1..];
    let fields: Vec<&str> = rest.split_whitespace().collect();
    // rest starts right after ')', so index 0 here == field 3 in /proc format.
    let utime: u64 = fields.get(11)?.parse().ok()?; // field 14
    let stime: u64 = fields.get(12)?.parse().ok()?; // field 15
    let starttime: u64 = fields.get(19)?.parse().ok()?; // field 22
    Some((utime + stime, starttime))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_pct_converts_jiffies_via_clk_tck() {
        // 300 ticks at 100 ticks/s = 3 CPU-seconds over 6s wall time on 4
        // cores: 3/6/4*100 = 12.5%. Before the CLK_TCK fix this computed
        // 1250% (ticks treated as raw seconds).
        let pct = compute_cpu_pct(300, 6.0, 4.0, 100.0);
        assert!((pct - 12.5).abs() < 1e-9, "got {}", pct);
    }

    #[test]
    fn cpu_pct_single_core_full_load_on_many_cores() {
        // 1000 ticks = 10 CPU-seconds in 10s wall = exactly one busy core.
        // On an 8-core machine that is 100%/8 = 12.5%.
        let pct = compute_cpu_pct(1000, 10.0, 8.0, 100.0);
        assert!((pct - 12.5).abs() < 1e-9, "got {}", pct);
    }

    #[test]
    fn cpu_pct_all_cores_saturated() {
        // 8000 ticks = 80 CPU-seconds in 10s on 8 cores: every core fully
        // busy. The formula normalizes over all cores, so this reads 100%.
        let pct = compute_cpu_pct(8000, 10.0, 8.0, 100.0);
        assert!((pct - 100.0).abs() < 1e-9, "got {}", pct);
    }

    #[test]
    fn cpu_pct_clamps_at_100_times_ncpu() {
        // Absurd jiffies delta (well beyond 8 fully busy cores) is clamped
        // to the historical 100*ncpu ceiling.
        let pct = compute_cpu_pct(800_000, 10.0, 8.0, 100.0);
        assert!((pct - 800.0).abs() < 1e-9, "got {}", pct);
    }

    #[test]
    fn cpu_pct_zero_on_idle_or_invalid_input() {
        assert_eq!(compute_cpu_pct(0, 10.0, 4.0, 100.0), 0.0);
        assert_eq!(compute_cpu_pct(100, 0.0, 4.0, 100.0), 0.0);
        assert_eq!(compute_cpu_pct(100, -1.0, 4.0, 100.0), 0.0);
        assert_eq!(compute_cpu_pct(100, 10.0, 4.0, 0.0), 0.0);
    }

    #[test]
    fn clk_tck_resolves_to_positive_value() {
        // On this environment sysconf must succeed (Linux); 100 is the
        // universal value, so just sanity-check bounds.
        let tck = clock_ticks_per_second();
        assert!(tck > 0.0 && tck.is_finite(), "got {}", tck);
    }

    #[test]
    fn proc_stat_parsing_handles_comm_with_parens_and_spaces() {
        // comm contains ')' and spaces; utime=250, stime=50 (fields 14/15),
        // starttime=12345 (field 22).
        let stat = "42 (firefox (web content)) R 1 0 0 0 0 0 0 0 0 0 250 50 0 0 0 0 0 0 12345";
        assert_eq!(parse_proc_stat(stat), Some((300, 12345)));
    }

    #[test]
    fn proc_stat_parsing_rejects_truncated_or_malformed_input() {
        assert_eq!(parse_proc_stat(""), None);
        assert_eq!(parse_proc_stat("42 (vim) R 1 2 3"), None);
        assert_eq!(parse_proc_stat("no closing paren 1 2 3"), None);
        // utime (field 14, index 11) is not a number here.
        assert_eq!(
            parse_proc_stat("7 (app) R x x x x x x x x x x x 2"),
            None
        );
    }
}
