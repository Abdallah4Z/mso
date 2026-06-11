/// Send SIGTERM to a process; if it doesn't exist, that's fine.
pub fn kill_process(pid: u32) {
    if pid == 0 {
        return;
    }
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
}

/// Send SIGKILL (force kill).
pub fn force_kill(pid: u32) {
    if pid == 0 {
        return;
    }
    unsafe {
        libc::kill(pid as i32, libc::SIGKILL);
    }
}

/// Check if a process is still alive (not a zombie).
pub fn is_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    // kill(pid, 0) succeeds for zombies too, so check /proc/[pid]/status
    if let Ok(status) = std::fs::read_to_string(format!("/proc/{pid}/status")) {
        for line in status.lines() {
            if line.starts_with("State:") {
                // State: Z (zombie) means the process has exited but not been reaped
                return !line.contains('Z');
            }
        }
    }
    // Fallback: if /proc doesn't exist, process definitely doesn't exist
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kill_invalid_pid_does_not_crash() {
        kill_process(99999999);
        force_kill(99999999);
    }

    #[test]
    fn test_is_alive_zero_pid() {
        assert!(!is_alive(0));
    }

    #[test]
    fn test_is_alive_nonexistent_pid() {
        assert!(!is_alive(99999999));
    }
}
