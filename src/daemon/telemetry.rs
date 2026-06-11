use crate::protocol::ProcessTelemetry;
use crate::daemon::state::ProcessTable;
use std::collections::HashMap;
use std::sync::Mutex;

/// Per-PID CPU accumulator values (replaces buggy shared static mut)
struct CpuAccum {
    prev_utime: u64,
    prev_stime: u64,
    prev_timestamp: u64,
}

type SparklineHistory = std::sync::LazyLock<Mutex<HashMap<u32, Vec<(f32, u64)>>>>;

static CPU_ACCUM: std::sync::LazyLock<Mutex<HashMap<u32, CpuAccum>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

static SPARKLINE_HISTORY: SparklineHistory =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

const SPARKLINE_SAMPLES: usize = 60;

pub async fn telemetry_loop(table: ProcessTable) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
    loop {
        interval.tick().await;
        let now = crate::util::epoch_millis();
        for mut entry in table.inner.iter_mut() {
            let pid = entry.pid;
            if pid == 0 {
                continue;
            }
            let telemetry = collect_telemetry(pid);
            entry.cpu_percent = telemetry.cpu_percent;
            entry.memory_bytes = telemetry.memory_bytes;
            entry.ports = telemetry.ports;
            entry.io_bytes = telemetry.io_bytes;
            if entry.status == crate::protocol::ProcessStatus::Running {
                entry.uptime_secs = (now - entry.started_at) / 1000;
            }

            // Update sparkline history
            if let Ok(mut history) = SPARKLINE_HISTORY.lock() {
                let samples = history.entry(pid).or_default();
                samples.push((telemetry.cpu_percent, telemetry.memory_bytes));
                if samples.len() > SPARKLINE_SAMPLES {
                    samples.remove(0);
                }
                entry.sparkline_cpu = render_sparkline(samples.iter().map(|(cpu, _)| *cpu), 12);
                entry.sparkline_mem = render_sparkline(samples.iter().map(|(_, mem)| *mem as f32), 12);
            }
        }
    }
}

fn render_sparkline(values: impl Iterator<Item = f32>, width: usize) -> String {
    let vals: Vec<f32> = values.collect();
    if vals.is_empty() {
        return String::new();
    }
    let max = vals.iter().cloned()
        .filter(|v| v.is_finite())
        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or(1.0);
    if max <= 0.0 {
        return " ".repeat(width);
    }
    let bars = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
    vals.iter().rev().take(width).rev().map(|v| {
        let idx = ((v / max) * (bars.len() - 1) as f32).round() as usize;
        bars[idx.min(bars.len() - 1)]
    }).collect()
}

fn collect_telemetry(pid: u32) -> ProcessTelemetry {
    let mut ports = Vec::new();
    let mut cpu_percent = 0.0f32;
    let mut memory_bytes = 0u64;
    let mut io_bytes = 0u64;

    // Cache /proc paths to avoid repeated allocations
    let io_path = format!("/proc/{pid}/io");
    let statm_path = format!("/proc/{pid}/statm");
    let stat_path = format!("/proc/{pid}/stat");

    // ── IO: /proc/[pid]/io (rchar + wchar) ──────────────────
    if let Ok(io_data) = std::fs::read_to_string(&io_path) {
        for line in io_data.lines() {
            if let Some(val) = line.strip_prefix("rchar: ") {
                io_bytes += val.trim().parse::<u64>().unwrap_or(0);
            } else if let Some(val) = line.strip_prefix("wchar: ") {
                io_bytes += val.trim().parse::<u64>().unwrap_or(0);
            }
        }
    }

    // ── RAM: /proc/[pid]/statm ──────────────────────────────
    if let Ok(statm) = std::fs::read_to_string(&statm_path) {
        if let Some(rss_pages) = statm.split_whitespace().nth(1) {
            if let Ok(pages) = rss_pages.parse::<u64>() {
                let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as u64;
                memory_bytes = pages * page_size;
            }
        }
    }

    // ── CPU: /proc/[pid]/stat (fields 13-14 = utime, stime) ─
    if let Ok(stat) = std::fs::read_to_string(&stat_path) {
        let fields: Vec<&str> = stat.split_whitespace().collect();
        if fields.len() > 14 {
            if let (Ok(utime), Ok(stime)) = (
                fields[13].parse::<u64>(),
                fields[14].parse::<u64>(),
            ) {
                let now = crate::util::epoch_millis();
                let mut accum = CPU_ACCUM.lock().unwrap_or_else(|e| e.into_inner());
                let entry = accum.entry(pid).or_insert(CpuAccum { prev_utime: 0, prev_stime: 0, prev_timestamp: 0 });
                let delta_cpu = (utime.saturating_sub(entry.prev_utime)) + (stime.saturating_sub(entry.prev_stime));
                let delta_time = now.saturating_sub(entry.prev_timestamp);
                if delta_time > 0 {
                    let ticks_per_sec = sysconf_ticks_per_sec();
                    let cpu = (delta_cpu as f64 / ticks_per_sec as f64)
                        / (delta_time as f64 / 1000.0)
                        * 100.0;
                    cpu_percent = (cpu as f32).clamp(0.0, 100.0);
                }
                entry.prev_utime = utime;
                entry.prev_stime = stime;
                entry.prev_timestamp = now;
            }
        }
    }

    // ── Ports: /proc/net/tcp + /proc/net/tcp6 ───────────────
    ports.extend(parse_proc_net_tcp("/proc/net/tcp"));
    ports.extend(parse_proc_net_tcp("/proc/net/tcp6"));

    ProcessTelemetry { ports, cpu_percent, memory_bytes, io_bytes }
}

fn sysconf_ticks_per_sec() -> u64 {
    unsafe { libc::sysconf(libc::_SC_CLK_TCK) as u64 }
}

/// Parse /proc/net/tcp{,6} to extract locally-bound port numbers.
/// State 0A = LISTEN, local_address format is HEX_IP:HEX_PORT
fn parse_proc_net_tcp(path: &str) -> Vec<u16> {
    let mut ports = Vec::new();
    let Ok(content) = std::fs::read_to_string(path) else {
        return ports;
    };

    for line in content.lines().skip(1) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 4 {
            continue;
        }
        if fields[3] != "0A" {
            continue;
        }
        if let Some((_ip, port_hex)) = fields[1].rsplit_once(':') {
            if let Ok(port) = u16::from_str_radix(port_hex, 16) {
                ports.push(port);
            }
        }
    }
    ports
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_sparkline_empty() {
        assert_eq!(render_sparkline(std::iter::empty::<f32>(), 12), "");
    }

    #[test]
    fn test_render_sparkline_single() {
        assert_eq!(render_sparkline(vec![50.0].into_iter(), 5), "█");
    }

    #[test]
    fn test_render_sparkline_all_same() {
        let r = render_sparkline(vec![42.0, 42.0, 42.0].into_iter(), 3);
        assert_eq!(r, "███");
    }

    #[test]
    fn test_render_sparkline_zero() {
        assert_eq!(render_sparkline(vec![0.0, 0.0].into_iter(), 3), "   ");
    }

    #[test]
    fn test_render_sparkline_nan_filtered() {
        // NaN is present in vals but max_by filters it -> max=20
        // NaN/20 maps to ▁ (index 0), 10/20=0.5 -> ▅ (index 4), 20/20=1 -> █
        let r = render_sparkline(vec![f32::NAN, 10.0, 20.0].into_iter(), 3);
        assert_eq!(r, "▁▅█");
    }

    #[test]
    fn test_collect_telemetry_invalid_pid() {
        let t = collect_telemetry(99999999);
        assert_eq!(t.cpu_percent, 0.0);
        assert_eq!(t.memory_bytes, 0);
        // ports are system-wide (/proc/net/tcp), not per-PID, so may not be empty
    }
}