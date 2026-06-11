pub mod listener;
pub mod state;
pub mod supervisor;
pub mod persist;
pub mod telemetry;
pub mod signal;
pub mod log_db;
pub mod prometheus;

use crate::util;
use state::ProcessTable;
use log_db::LogDb;
use std::sync::Arc;
use anyhow::Context;
use std::os::unix::fs::PermissionsExt;

pub type LogBroadcast = tokio::sync::broadcast::Sender<(uuid::Uuid, crate::protocol::StreamKind, String, u64)>;

pub async fn run() -> anyhow::Result<()> {
    util::ensure_mso_dir()?;

    // Initalize SQLite log database
    let log_db = Arc::new(LogDb::open().context("opening log database")?);

    // Clean up stale socket
    let sock_path = util::mso_sock_path();
    let _ = std::fs::remove_file(&sock_path);

    let listener = tokio::net::UnixListener::bind(&sock_path)?;
    // Restrict socket to owner only
    if let Err(e) = std::fs::set_permissions(&sock_path, std::fs::Permissions::from_mode(0o700)) {
        tracing::warn!("failed to set socket permissions: {e}");
    }
    let process_table = ProcessTable::new();

    // Load persisted processes from last daemon run
    let saved = persist::load();
    if !saved.is_empty() {
        tracing::info!("loaded {} persisted processes (marked as crashed)", saved.len());
        for proc in saved {
            process_table.inner.insert(proc.id, proc);
        }
    }

    // Create log broadcast channel — multiple clients (TUI, runner) can subscribe
    let (log_tx, _) = tokio::sync::broadcast::channel::<(uuid::Uuid, crate::protocol::StreamKind, String, u64)>(1024);

    // Create the supervisor
    let supervisor = Arc::new(supervisor::ProcessSupervisor::new(
        process_table.clone(),
        log_tx.clone(),
        log_db.clone(),
    ));

    tracing::info!(pid = std::process::id(), "daemon started on {:?}", sock_path);

    // Spawn the telemetry collection loop
    let table_clone = process_table.clone();
    tokio::spawn(async move {
        telemetry::telemetry_loop(table_clone).await;
    });

    // Spawn Prometheus metrics server on port 9753 (configurable via MSO_METRICS_PORT)
    let metrics_port = std::env::var("MSO_METRICS_PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(9753);
    let table_prom = process_table.clone();
    tokio::spawn(async move {
        prometheus::serve_metrics(table_prom, metrics_port).await;
    });

    // Spawn auto-prune task — delete old logs every hour
    let cfg = crate::util::Config::load();
    let retention_days = cfg.log_retention_days.max(1);
    let log_db_prune = log_db.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
            let cutoff = crate::util::epoch_millis() - (retention_days * 86400 * 1000);
            if let Ok(count) = log_db_prune.prune_before(cutoff, None) {
                if count > 0 {
                    tracing::info!("auto-pruned {count} old log entries");
                }
            }
        }
    });

    // Spawn periodic auto-save — persists process table every 30s
    let table_autosave = process_table.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            let procs: Vec<_> = table_autosave.inner.iter().map(|e| e.value().clone()).collect();
            crate::daemon::persist::save(&procs);
        }
    });

    // Register signal handler for graceful shutdown
    let mut sig_term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .context("registering SIGTERM handler")?;
    let mut sig_int = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
        .context("registering SIGINT handler")?;
    let supervisor_shutdown = supervisor.clone();
    let sock_path_clone = sock_path.clone();
    tokio::spawn(async move {
        tokio::select! {
            _ = sig_term.recv() => {}
            _ = sig_int.recv() => {}
        }
        tracing::info!("shutdown signal received, terminating children...");
        supervisor_shutdown.shutdown_all(std::time::Duration::from_secs(3)).await;
        let _ = std::fs::remove_file(&sock_path_clone);
        let _ = std::fs::remove_file(crate::util::mso_pid_path());
        std::process::exit(0);
    });

    // Accept loop
    listener::accept_loop(listener, supervisor, log_tx).await
}
