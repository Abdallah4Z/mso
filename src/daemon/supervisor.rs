use crate::daemon::state::ProcessTable;
use crate::daemon::log_db::LogDb;
use crate::protocol::{ManagedProcess, ProcessRegistration, ProcessStatus, RestartPolicy, StreamKind, TimestampedLine};
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use uuid::Uuid;

const EXIT_MONITOR_SECS: u64 = 2;
const KILL_TIMEOUT_SECS: u64 = 2;
const INIT_DELAY_MS: u64 = 100;
const PIPE_BUF_SIZE: usize = 4096;

pub type LogBroadcaster = tokio::sync::broadcast::Sender<(Uuid, StreamKind, String, u64)>;

pub struct ProcessSupervisor {
    pub table: ProcessTable,
    children: Arc<dashmap::DashMap<Uuid, Child>>,
    pub log_tx: LogBroadcaster,
    pub log_db: Arc<LogDb>,
}

impl ProcessSupervisor {
    pub fn new(table: ProcessTable, log_tx: LogBroadcaster, log_db: Arc<LogDb>) -> Self {
        Self { table, children: Arc::new(dashmap::DashMap::new()), log_tx, log_db }
    }

    pub async fn spawn(&self, id: Uuid, reg: &ProcessRegistration) -> anyhow::Result<u32> {
        let mut child = Command::new(&reg.command[0])
            .args(&reg.command[1..]).current_dir(&reg.working_dir)
            .env_clear().envs(&reg.env_vars)
            .stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped())
            .spawn().map_err(|e| anyhow::anyhow!("failed to spawn {}: {e}", reg.command[0]))?;
        let pid = child.id().unwrap_or(0);

        let proc = ManagedProcess::from_registration(id, pid, reg);
        self.table.inner.insert(id, proc);

        let out = child.stdout.take();
        let err = child.stderr.take();
        let t = self.table.clone(); let lx = self.log_tx.clone(); let lb = self.log_db.clone();
        tokio::spawn(async move {
            if let Some(r) = out { pipe_reader(id, r, StreamKind::Stdout, t.clone(), lx.clone(), lb.clone()).await; }
            if let Some(r) = err { pipe_reader(id, r, StreamKind::Stderr, t, lx, lb).await; }
        });

        self.children.insert(id, child);

        // Exit monitor
        let em = ExitMonitor {
            id, table: self.table.clone(), children: self.children.clone(),
            table2: self.table.clone(), children2: self.children.clone(),
            log_tx: self.log_tx.clone(), log_db: self.log_db.clone(),
        };
        tokio::spawn(async move { em.run().await });

        // Health check loop
        if let Some(ref hc) = reg.health_check {
            let hc_id = id;
            let hc_config = hc.clone();
            let hc_table = self.table.clone();
            let hc_supervisor_table = self.table.clone();
            let hc_supervisor_children = self.children.clone();
            let hc_log_tx = self.log_tx.clone();
            tokio::spawn(async move {
                let mut failures = 0u32;
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(hc_config.interval_secs)).await;
                    let ok = tokio::time::timeout(
                        std::time::Duration::from_secs(hc_config.timeout_secs),
                        reqwest::get(&hc_config.url),
                    ).await.ok().and_then(|r| r.ok()).is_some();
                    if ok { failures = 0; } else { failures += 1; }
                    if let Some(mut proc) = hc_table.inner.get_mut(&hc_id) { proc.health_ok = ok; }
                    let _ = hc_log_tx.send((hc_id, StreamKind::Stdout, format!("health: {} (failures={})", if ok { "OK" } else { "FAIL" }, failures), crate::util::epoch_millis()));
                    if failures >= hc_config.max_failures {
                        tracing::warn!(process_id = %hc_id, "health check failed {} times, restarting", failures);
                        if let Some((_id, mut child)) = hc_supervisor_children.remove(&hc_id) {
                            crate::daemon::signal::kill_process(child.id().unwrap_or(0));
                            let _ = child.wait().await;
                        }
                        if let Some(mut proc) = hc_supervisor_table.inner.get_mut(&hc_id) {
                            proc.status = ProcessStatus::Stopped;
                        }
                        break;
                    }
                }
            });
        }
        Ok(pid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{ProcessRegistration, ProcessStatus, RestartPolicy};
    use std::time::Duration;
    use tokio::sync::broadcast;

    fn test_supervisor() -> (ProcessSupervisor, ProcessTable, Arc<LogDb>) {
        let table = ProcessTable::new();
        let db = Arc::new(LogDb::open_in_memory().unwrap());
        let (tx, _) = broadcast::channel::<(Uuid, StreamKind, String, u64)>(16);
        let sup = ProcessSupervisor::new(table.clone(), tx, db.clone());
        (sup, table, db)
    }

    fn test_reg(cmd: &[&str]) -> ProcessRegistration {
        ProcessRegistration {
            pid: 0,
            command: cmd.iter().map(|s| s.to_string()).collect(),
            working_dir: std::env::temp_dir(),
            env_vars: std::collections::HashMap::new(),
            silence_duration: None,
            restart_policy: RestartPolicy::No,
            tags: Vec::new(),
            health_check: None,
            log_file: None,
        }
    }

    #[ignore = "spawns real processes (slow)"]
    #[tokio::test]
    async fn test_spawn_echo() {
        let (sup, table, _) = test_supervisor();
        let id = Uuid::new_v4();
        let reg = test_reg(&["echo", "hello"]);
        let pid = sup.spawn(id, &reg).await.unwrap();
        assert!(pid > 0);
        tokio::time::sleep(Duration::from_millis(500)).await;
        let proc = table.inner.get(&id).unwrap();
        assert_eq!(proc.status, ProcessStatus::Stopped);
    }

    #[ignore = "spawns real processes (slow)"]
    #[tokio::test]
    async fn test_spawn_and_alive() {
        let (sup, table, _) = test_supervisor();
        let id = Uuid::new_v4();
        let reg = test_reg(&["sleep", "10"]);
        let pid = sup.spawn(id, &reg).await.unwrap();
        assert!(pid > 0);
        tokio::time::sleep(Duration::from_millis(200)).await;
        let proc = table.inner.get(&id).unwrap();
        assert_eq!(proc.status, ProcessStatus::Running);
        sup.kill(id).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        let proc = table.inner.get(&id).unwrap();
        assert_eq!(proc.status, ProcessStatus::Stopped);
    }

    #[ignore = "spawns real processes (slow)"]
    #[tokio::test]
    async fn test_restart() {
        let (sup, table, _) = test_supervisor();
        let id = Uuid::new_v4();
        let reg = test_reg(&["echo", "restart-test"]);
        sup.spawn(id, &reg).await.unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;
        sup.restart(id).await.unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;
        let stopped = table.inner.iter().filter(|e| e.value().status == ProcessStatus::Stopped).count();
        assert_eq!(stopped, 1);
    }

    #[ignore = "spawns real processes (slow)"]
    #[tokio::test]
    async fn test_exit_monitor_polls() {
        let (sup, table, _) = test_supervisor();
        let id = Uuid::new_v4();
        let reg = test_reg(&["sleep", "1"]);
        sup.spawn(id, &reg).await.unwrap();
        tokio::time::sleep(Duration::from_secs(3)).await;
        let proc = table.inner.get(&id).unwrap();
        assert_eq!(proc.status, ProcessStatus::Stopped);
    }

    #[ignore = "spawns real processes (slow)"]
    #[tokio::test]
    async fn test_auto_restart() {
        let (sup, table, _) = test_supervisor();
        let id = Uuid::new_v4();
        let mut reg = test_reg(&["echo", "auto-restart"]);
        reg.restart_policy = RestartPolicy::Always;
        let _pid1 = sup.spawn(id, &reg).await.unwrap();
        tokio::time::sleep(Duration::from_secs(4)).await;
        let count = table.inner.iter().count();
        assert!(count >= 2);
    }
}

impl ProcessSupervisor {
    pub async fn shutdown_all(&self, grace: std::time::Duration) {
        let ids: Vec<Uuid> = self.children.iter().map(|e| *e.key()).collect();
        for id in &ids {
            tracing::info!(process_id = %id, "sending SIGTERM for shutdown");
            if let Err(e) = self.kill(*id).await { tracing::error!(process_id = %id, "shutdown kill failed: {e}"); }
        }
        tokio::time::sleep(grace).await;
        for id in self.table.inner.iter().filter(|e| e.value().status == ProcessStatus::Running).map(|e| *e.key()).collect::<Vec<_>>() {
            if let Some(proc) = self.table.inner.get(&id) { if proc.pid > 0 { crate::daemon::signal::force_kill(proc.pid); } }
            if let Some(mut proc) = self.table.inner.get_mut(&id) { proc.status = ProcessStatus::Stopped; }
        }
        let procs: Vec<_> = self.table.inner.iter().map(|e| e.value().clone()).collect();
        crate::daemon::persist::save(&procs);
        tracing::info!("shutdown complete");
    }

    pub async fn kill(&self, id: Uuid) -> anyhow::Result<()> {
        if let Some((_id, mut child)) = self.children.remove(&id) {
            if let Some(pid) = child.id() { crate::daemon::signal::kill_process(pid); }
            let result = tokio::time::timeout(std::time::Duration::from_secs(KILL_TIMEOUT_SECS), child.wait()).await;
            match result {
                Ok(Ok(status)) => tracing::info!(process_id = %id, exit_code = ?status.code(), "process killed (SIGTERM)"),
                _ => {
                    if let Some(pid) = child.id() { crate::daemon::signal::force_kill(pid); let _ = child.wait().await; }
                    tracing::info!(process_id = %id, "process force-killed (SIGKILL)");
                }
            }
        } else if let Some(proc) = self.table.inner.get(&id) {
            if proc.pid > 0 { crate::daemon::signal::kill_process(proc.pid); }
        }
        if let Some(mut proc) = self.table.inner.get_mut(&id) { proc.status = ProcessStatus::Stopped; }
        Ok(())
    }

    pub async fn restart(&self, id: Uuid) -> anyhow::Result<()> {
        let reg = {
            let proc = self.table.inner.get(&id).ok_or_else(|| anyhow::anyhow!("process {id} not found"))?;
            ProcessRegistration {
                pid: 0,
                command: proc.command.clone(),
                working_dir: proc.working_dir.clone(),
                env_vars: proc.env_vars.clone(),
                silence_duration: None,
                restart_policy: proc.restart_policy,
                tags: proc.tags.clone(),
                health_check: proc.health_check.clone(),
                log_file: proc.log_file.clone(),
            }
        };
        self.kill(id).await?;
        self.table.inner.remove(&id);
        let new_id = Uuid::new_v4();
        let pid = self.spawn(new_id, &reg).await?;
        tracing::info!(old_id = %id, new_id = %new_id, pid, "process restarted");
        Ok(())
    }
}

// ── Auto-restart spawn (used by ExitMonitor) ──

struct ExitMonitor {
    id: Uuid, table: ProcessTable, children: Arc<dashmap::DashMap<Uuid, Child>>,
    table2: ProcessTable, children2: Arc<dashmap::DashMap<Uuid, Child>>,
    log_tx: LogBroadcaster, log_db: Arc<LogDb>,
}

impl ExitMonitor {
    async fn run(&self) {
        tokio::time::sleep(std::time::Duration::from_millis(INIT_DELAY_MS)).await;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(EXIT_MONITOR_SECS)).await;
            let alive = self.table.inner.get(&self.id)
                .filter(|p| p.status == ProcessStatus::Running)
                .map(|p| crate::daemon::signal::is_alive(p.pid))
                .unwrap_or(false);
            if alive { continue; }

            let should_restart = self.table.inner.get(&self.id)
                .map(|p| p.restart_policy == RestartPolicy::Always)
                .unwrap_or(false);

            if should_restart {
                if let Some(proc) = self.table.inner.get(&self.id) {
                    let reg = ProcessRegistration {
                        pid: 0, command: proc.command.clone(), working_dir: proc.working_dir.clone(),
                        env_vars: proc.env_vars.clone(), silence_duration: None,
                        restart_policy: RestartPolicy::Always,
                        tags: proc.tags.clone(), health_check: proc.health_check.clone(),
                        log_file: proc.log_file.clone(),
                    };
                    drop(proc);
                    if let Some((_, mut old)) = self.children.remove(&self.id) { let _ = old.wait().await; }
                    let new_id = Uuid::new_v4();
                    let em2 = ExitMonitor {
                        id: new_id, table: self.table2.clone(), children: self.children2.clone(),
                        table2: self.table2.clone(), children2: self.children2.clone(),
                        log_tx: self.log_tx.clone(), log_db: self.log_db.clone(),
                    };
                    tokio::spawn(async move {
                        if let Err(e) = restart_spawn(new_id, &reg, &em2.table, &em2.children, &em2.log_tx, &em2.log_db).await {
                            tracing::error!("auto-restart failed: {e}");
                        }
                    });
                }
            } else {
                if let Some(mut proc) = self.table.inner.get_mut(&self.id) { proc.status = ProcessStatus::Stopped; }
                if let Some((_, mut child)) = self.children.remove(&self.id) { let _ = child.wait().await; }
                let procs: Vec<_> = self.table.inner.iter().map(|e| e.value().clone()).collect();
                crate::daemon::persist::save(&procs);
                tracing::info!(process_id = %self.id, "process exited (monitor)");
            }
            break;
        }
    }
}

async fn restart_spawn(
    id: Uuid, reg: &ProcessRegistration,
    table: &ProcessTable, children: &dashmap::DashMap<Uuid, Child>,
    log_tx: &LogBroadcaster, log_db: &Arc<LogDb>,
) -> anyhow::Result<()> {
    let mut child = Command::new(&reg.command[0])
        .args(&reg.command[1..]).current_dir(&reg.working_dir)
        .env_clear().envs(&reg.env_vars)
        .stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped())
        .spawn().map_err(|e| anyhow::anyhow!("auto-restart spawn failed: {e}"))?;
    let pid = child.id().unwrap_or(0);

    let proc = ManagedProcess::from_registration(id, pid, reg);
    table.inner.insert(id, proc);

    let out = child.stdout.take();
    let err = child.stderr.take();
    let t = table.clone(); let lx = log_tx.clone(); let lb = log_db.clone();
    tokio::spawn(async move {
        if let Some(r) = out { pipe_reader(id, r, StreamKind::Stdout, t.clone(), lx.clone(), lb.clone()).await; }
        if let Some(r) = err { pipe_reader(id, r, StreamKind::Stderr, t, lx, lb).await; }
    });

    children.insert(id, child);
    Ok(())
}

async fn pipe_reader(
    process_id: Uuid, mut reader: impl Unpin + tokio::io::AsyncRead,
    stream: StreamKind, table: ProcessTable,
    log_tx: LogBroadcaster, log_db: Arc<LogDb>,
) {
    let mut buf = Vec::with_capacity(PIPE_BUF_SIZE);
    loop {
        buf.clear();
        match reader.read_buf(&mut buf).await {
            Ok(0) => break,
            Ok(_) => {
                let line = String::from_utf8_lossy(&buf).to_string();
                let ts = crate::util::epoch_millis();
                if let Some(mut proc) = table.inner.get_mut(&process_id) {
                    if proc.log_lines.len() >= crate::protocol::MAX_LOG_LINES { proc.log_lines.pop_front(); }
                    proc.log_lines.push_back(TimestampedLine { timestamp: ts, line: line.clone() });
                }
                if let Err(e) = log_db.insert(process_id, stream as i32, &line, ts) {
                    tracing::warn!("log db insert failed: {e}");
                }
                let _ = log_tx.send((process_id, stream, line, ts));
            }
            Err(_) => break,
        }
    }
}
