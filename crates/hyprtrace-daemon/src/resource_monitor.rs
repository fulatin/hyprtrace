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

            // One usable sample of the process: CPU ticks, start time (for
            // PID-reuse detection) and RSS. A process that is gone, or that
            // reports no RSS line (kernel threads), is skipped entirely rather
            // than being recorded as 0 MB.
            let (jiffies, starttime, mem_kb) = match sample_process(pid) {
                Some(sample) => sample,
                None => {
                    last = None;
                    continue;
                }
            };
            let now = std::time::Instant::now();

            let cpu_pct = cpu_pct_since(
                last.as_ref(),
                pid,
                starttime,
                jiffies,
                now,
                ncpu,
                clk_tck,
            );

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

/// One CPU sample: `(pid, process start time, utime+stime jiffies, wall time)`.
///
/// `starttime` comes from `/proc/<pid>/stat` and is what makes PID reuse
/// detectable: a recycled PID has the same id but a different start time.
type CpuSample = (i32, u64, u64, std::time::Instant);

/// CPU percentage between the previous sample and this one.
///
/// Returns `0.0` when there is no usable previous sample, when the PID
/// changed, or when the PID was reused by a new process (start time differs).
/// In the last two cases the jiffies delta belongs to a different process and
/// must not be attributed to the one tracked now (issue #21).
fn cpu_pct_since(
    last: Option<&CpuSample>,
    pid: i32,
    starttime: u64,
    jiffies: u64,
    now: std::time::Instant,
    ncpu: f64,
    clk_tck: f64,
) -> f64 {
    match last {
        Some((last_pid, last_starttime, last_jiffies, last_at))
            if *last_pid == pid && *last_starttime == starttime =>
        {
            let dj = jiffies.saturating_sub(*last_jiffies);
            let dt = now.duration_since(*last_at).as_secs_f64();
            compute_cpu_pct(dj, dt, ncpu, clk_tck)
        }
        _ => 0.0,
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

/// One usable resource sample: `(utime+stime jiffies, starttime, RSS KB)`.
///
/// Returns `None` when either half is unavailable. The RSS half matters on its
/// own: kernel threads (`kthreadd`, `kworker/*`) have a `/proc/<pid>/stat` but
/// no `VmRSS` line in `/proc/<pid>/status`, and recording them as 0 MB used to
/// pollute the resource statistics (issue #21).
fn sample_process(pid: i32) -> Option<(u64, u64, i64)> {
    combine_sample(read_proc_sample(pid), read_proc_mem_kb(pid))
}

/// Join the two `/proc` readings, refusing a sample when the RSS reading is
/// missing. Split out so the "no RSS line means skip, not 0" rule is testable
/// without a real kernel thread.
fn combine_sample(sample: Option<(u64, u64)>, rss_kb: Option<i64>) -> Option<(u64, u64, i64)> {
    let (jiffies, starttime) = sample?;
    Some((jiffies, starttime, rss_kb?))
}

/// Sample CPU ticks + process start time via procfs. Type-safe `/proc` parsing
/// (handles a comm field with spaces/parens, and reads fields by name instead
/// of a hard-coded index). Returns `(jiffies, starttime)`; `starttime` is used
/// to detect PID reuse.
fn read_proc_sample(pid: i32) -> Option<(u64, u64)> {
    let proc = procfs::process::Process::new(pid).ok()?;
    let stat = proc.stat().ok()?;
    Some((stat.utime + stat.stime, stat.starttime))
}

/// RSS in KB from /proc/<pid>/status VmRSS.
/// RSS in KB via procfs. Returns `None` when the process reports no RSS line
/// (e.g. a kernel thread), which the caller skips rather than recording 0.
fn read_proc_mem_kb(pid: i32) -> Option<i64> {
    let proc = procfs::process::Process::new(pid).ok()?;
    let status = proc.status().ok()?;
    status.vmrss.map(|kb| kb as i64)
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
    fn read_proc_sample_reads_current_process() {
        // procfs parses the real /proc/<pid>/stat; sanity-check it returns a
        // jiffies+starttime pair for this process. CPU ticks may be 0 for a
        // process that has not been scheduled yet, so only assert on starttime.
        let pid = std::process::id() as i32;
        let (_jiffies, starttime) = read_proc_sample(pid).expect("current process is readable");
        assert!(starttime > 0, "starttime is positive");
    }

    #[test]
    fn cpu_pct_uses_the_previous_sample_for_the_same_process() {
        let t0 = std::time::Instant::now();
        // 50 jiffies over 1s at CLK_TCK=100 on one CPU = 50% CPU.
        let last: CpuSample = (4242, 900_000, 100, t0);
        let pct = cpu_pct_since(
            Some(&last),
            4242,
            900_000,
            150,
            t0 + std::time::Duration::from_secs(1),
            1.0,
            100.0,
        );
        assert!((pct - 50.0).abs() < 0.001, "got {pct}");
    }

    #[test]
    fn reused_pid_does_not_attribute_the_new_process_cpu() {
        // Issue #21: the tracked process exited and its PID was reused by a
        // different process (different start time). The jiffies delta between
        // the two samples belongs to two different processes, so the sample
        // must report 0 rather than borrow the new process's CPU time.
        let t0 = std::time::Instant::now();
        let last: CpuSample = (4242, 900_000, 100, t0);
        let pct = cpu_pct_since(
            Some(&last),
            4242,
            1_200_000,
            900,
            t0 + std::time::Duration::from_secs(30),
            8.0,
            100.0,
        );
        assert_eq!(pct, 0.0);
    }

    #[test]
    fn another_pid_and_a_missing_previous_sample_report_no_cpu() {
        let t0 = std::time::Instant::now();
        let last: CpuSample = (1, 10, 100, t0);
        assert_eq!(
            cpu_pct_since(Some(&last), 2, 10, 900, t0, 4.0, 100.0),
            0.0,
            "a different pid must not inherit the sample"
        );
        assert_eq!(
            cpu_pct_since(None, 2, 10, 900, t0, 4.0, 100.0),
            0.0,
            "the first sample has no delta to report"
        );
    }

    #[test]
    fn a_process_without_an_rss_line_is_skipped() {
        // Kernel threads have `/proc/<pid>/stat` but no `VmRSS` line. They must
        // be skipped, not recorded as 0 MB (issue #21).
        assert_eq!(combine_sample(Some((100, 900_000)), None), None);
        // Without a /proc sample there is nothing to record either.
        assert_eq!(combine_sample(None, Some(4096)), None);
        // Both halves present: a complete sample.
        assert_eq!(
            combine_sample(Some((100, 900_000)), Some(4096)),
            Some((100, 900_000, 4096))
        );
        // A live pid that exposes no RSS line is skipped by the real reader.
        assert_eq!(sample_process(i32::MAX), None);
    }

    #[test]
    fn read_proc_sample_missing_pid_is_none() {
        // A very unlikely-to-exist pid returns None, not a panic.
        assert!(read_proc_sample(i32::MAX).is_none());
    }
}
