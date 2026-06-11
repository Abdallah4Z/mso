use crate::protocol::{ManagedProcess, ProcessStatus};
use crate::util;
use anyhow::Context;
use std::path::PathBuf;

fn processes_file() -> PathBuf {
    util::mso_dir().join("processes.json")
}

/// Save the full process table to disk. Retries up to 3 times on failure.
pub fn save(processes: &[ManagedProcess]) {
    for attempt in 1..=3 {
        match save_inner(processes) {
            Ok(()) => return,
            Err(e) => {
                if attempt < 3 {
                    std::thread::sleep(std::time::Duration::from_millis(100 * attempt));
                } else {
                    tracing::warn!("failed to persist processes after 3 attempts: {e}");
                }
            }
        }
    }
}

fn save_inner(processes: &[ManagedProcess]) -> anyhow::Result<()> {
    util::ensure_mso_dir()?;
    let json = serde_json::to_string_pretty(processes)
        .context("serializing process table")?;
    std::fs::write(processes_file(), json)
        .context("writing processes.json")?;
    Ok(())
}

/// Load the process table from disk.
/// Checks if each process's PID is still alive to determine its actual status.
pub fn load() -> Vec<ManagedProcess> {
    let path = processes_file();
    let Ok(data) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let Ok(mut processes) = serde_json::from_slice::<Vec<ManagedProcess>>(data.as_bytes()) else {
        return Vec::new();
    };
    for proc in &mut processes {
        if proc.status == ProcessStatus::Running {
            if proc.pid > 0 && crate::daemon::signal::is_alive(proc.pid) {
                proc.status = ProcessStatus::Running;
            } else {
                proc.status = ProcessStatus::Crashed;
                proc.pid = 0;
            }
        }
    }
    processes
}
