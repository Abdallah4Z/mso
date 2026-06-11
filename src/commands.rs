use crate::protocol;
use crate::util;
use clap::CommandFactory;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub struct RunConfig {
    pub silence: Option<u64>,
    pub restart: Option<String>,
    pub command: Vec<String>,
    pub tags: Vec<String>,
    pub health_check: Option<String>,
    pub health_interval: u64,
    pub health_timeout: u64,
    pub health_max_failures: u32,
    pub log_file: Option<String>,
    pub preset: Option<String>,
}

pub async fn handle_run(cfg: RunConfig) -> anyhow::Result<()> {
    let config = util::Config::load();
    let preset_vals = cfg.preset.as_ref().and_then(|n| util::load_preset(n));
    let preset_ref = preset_vals.as_ref();
    let effective_silence = cfg.silence.or(config.silence_secs).or(preset_ref.and_then(|p| p.silence_secs));
    let effective_restart = match cfg.restart {
        Some(v) => v,
        None => preset_ref.map(|p| p.restart_policy.clone()).unwrap_or(config.restart_policy.clone()),
    };
    let effective_tags = if !cfg.tags.is_empty() { cfg.tags } else { preset_ref.map(|p| p.tags.clone()).unwrap_or_default() };
    let hc_url = cfg.health_check.or(preset_ref.and_then(|p| p.health_check.clone()));
    let hc = hc_url.map(|url| protocol::HealthCheckConfig {
        url, interval_secs: cfg.health_interval, timeout_secs: cfg.health_timeout, max_failures: cfg.health_max_failures,
    });
    let policy = match effective_restart.as_str() { "always" => protocol::RestartPolicy::Always, _ => protocol::RestartPolicy::No };
    crate::client::runner::run_wrapped(cfg.command, effective_silence, policy, effective_tags, hc, cfg.log_file).await
}

pub async fn handle_view(readonly: bool) -> anyhow::Result<()> {
    crate::tui::run_with_options(readonly).await
}

pub async fn handle_view_default() -> anyhow::Result<()> {
    crate::tui::run().await
}

pub fn handle_completion(shell: clap_complete::Shell) -> anyhow::Result<()> {
    let mut cmd = crate::cli::Cli::command();
    let name = cmd.get_name().to_string();
    clap_complete::generate(shell, &mut cmd, name, &mut std::io::stdout());
    Ok(())
}

pub async fn handle_prune(days: u64, process: Option<String>) -> anyhow::Result<()> {
    let pid = process.as_ref().and_then(|s| uuid::Uuid::parse_str(s).ok());
    let msg = protocol::ClientMessage::PruneLogs { older_than_days: days, process_id: pid };
    send_and_print(msg, |resp| match resp {
        protocol::DaemonMessage::LogsPruned { count } => eprintln!("[mso] pruned {count} log entries"),
        protocol::DaemonMessage::Error(e) => eprintln!("[mso] prune failed: {e}"),
        _ => eprintln!("[mso] unexpected response"),
    }).await
}

pub async fn handle_logs(id: String, format: String, tail: Option<usize>) -> anyhow::Result<()> {
    crate::client::daemonize::ensure_daemon().await?;
    let procs = get_processes().await?;
    let proc = procs.iter().find(|p| p.id.to_string().starts_with(&id) || p.pid.to_string() == id)
        .ok_or_else(|| anyhow::anyhow!("no process matching '{id}'"))?;
    let limit = tail.unwrap_or(10000);
    let get_msg = protocol::ClientMessage::GetLogs { process_id: proc.id, offset: 0, limit };
    let wire2 = protocol::encode_message(&get_msg)?;
    let stream = crate::client::daemonize::connect_daemon().await?;
    let (mut reader, mut writer) = tokio::io::split(stream);
    writer.write_all(&wire2).await?;
    let mut len_buf2 = [0u8; 4]; reader.read_exact(&mut len_buf2).await?;
    let len2 = u32::from_le_bytes(len_buf2) as usize;
    let mut payload2 = vec![0u8; len2]; reader.read_exact(&mut payload2).await?;
    let batch: protocol::DaemonMessage = if payload2.len() >= 4 && payload2[0..4] == protocol::WIRE_MAGIC {
        bincode::deserialize(&payload2[4..])?
    } else { return Err(anyhow::anyhow!("bad response")); };
    if let protocol::DaemonMessage::LogsBatch { logs, total } = batch {
        let is_json = format == "json";
        for tl in &logs {
            if is_json { println!("{}", serde_json::to_string(tl).unwrap_or_default()); }
            else {
                let secs = tl.timestamp / 1000;
                let ts = format!("{:02}:{:02}:{:02}", (secs / 3600) % 24, (secs / 60) % 60, secs % 60);
                println!("[{}] {}", ts, tl.line);
            }
        }
        if !is_json { eprintln!("[mso] {} of {} log lines shown", logs.len(), total); }
    } else { eprintln!("[mso] unexpected response"); }
    Ok(())
}

pub fn handle_exec(command: Vec<String>) -> anyhow::Result<()> {
    let status = std::process::Command::new(&command[0]).args(&command[1..])
        .stdout(std::process::Stdio::inherit()).stderr(std::process::Stdio::inherit())
        .stdin(std::process::Stdio::inherit()).spawn()
        .map_err(|e| anyhow::anyhow!("failed to spawn {}: {e}", command[0]))?
        .wait().map_err(|e| anyhow::anyhow!("failed to wait: {e}"))?;
    std::process::exit(status.code().unwrap_or(1));
}

pub fn handle_config(action: String) -> anyhow::Result<()> {
    match action.as_str() {
        "validate" => {
            let cfg = util::Config::load();
            let errors = cfg.validate();
            if errors.is_empty() { eprintln!("[mso] ✓ configuration is valid"); }
            else { for err in &errors { eprintln!("[mso] ✗ {err}"); } }
        }
        "show" => match std::fs::read_to_string(util::mso_config_path()) {
            Ok(c) => print!("{c}"),
            Err(_) => eprintln!("[mso] no config file at {}", util::mso_config_path().display()),
        },
        "path" => println!("{}", util::mso_config_path().display()),
        _ => eprintln!("[mso] unknown action: {action} (try: validate, show, path)"),
    }
    Ok(())
}

pub async fn handle_stats(format: String) -> anyhow::Result<()> {
    crate::client::daemonize::ensure_daemon().await?;
    let processes = get_processes().await?;
    if format == "json" {
        println!("{}", serde_json::to_string_pretty(&processes)?);
    } else {
        println!("{:<8} {:<20} {:>6} {:>10} {:>12} STATUS", "PID", "NAME", "CPU%", "MEM", "PORTS");
        for p in &processes {
            let name = p.command.first().map(|s| s.as_str()).unwrap_or("?");
            let mem = crate::tui::theme::human_bytes(p.memory_bytes);
            let ports: String = if p.ports.is_empty() { "-".into() } else { p.ports.iter().map(|x| format!(":{}", x)).collect::<Vec<_>>().join(" ") };
            println!("{:<8} {:<20} {:>5.0}% {:>10} {:>12} {:?}", p.pid, name, p.cpu_percent, mem, ports, p.status);
        }
    }
    Ok(())
}

pub async fn handle_daemon() -> anyhow::Result<()> {
    util::ensure_mso_dir()?;
    util::write_daemon_pid(std::process::id())?;
    crate::daemon::run().await
}

// ── Helpers ──

async fn get_processes() -> anyhow::Result<Vec<crate::protocol::ManagedProcess>> {
    let msg = protocol::ClientMessage::GetState;
    let wire = protocol::encode_message(&msg)?;
    let stream = crate::client::daemonize::connect_daemon().await?;
    let (mut reader, mut writer) = tokio::io::split(stream);
    writer.write_all(&wire).await?;
    let mut len_buf = [0u8; 4]; reader.read_exact(&mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf) as usize;
    let mut payload = vec![0u8; len]; reader.read_exact(&mut payload).await?;
    let state: protocol::DaemonMessage = if payload.len() >= 4 && payload[0..4] == protocol::WIRE_MAGIC {
        bincode::deserialize(&payload[4..])?
    } else { return Err(anyhow::anyhow!("bad response")); };
    match state {
        protocol::DaemonMessage::StateSnapshot(snap) => Ok(snap.processes),
        _ => Err(anyhow::anyhow!("unexpected response")),
    }
}

async fn send_and_print<F: Fn(protocol::DaemonMessage)>(msg: protocol::ClientMessage, handler: F) -> anyhow::Result<()> {
    crate::client::daemonize::ensure_daemon().await?;
    let wire = protocol::encode_message(&msg)?;
    let stream = crate::client::daemonize::connect_daemon().await?;
    let (mut reader, mut writer) = tokio::io::split(stream);
    writer.write_all(&wire).await?;
    let mut len_buf = [0u8; 4]; reader.read_exact(&mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf) as usize;
    let mut payload = vec![0u8; len]; reader.read_exact(&mut payload).await?;
    if payload.len() >= 4 && payload[0..4] == protocol::WIRE_MAGIC {
        let resp: protocol::DaemonMessage = bincode::deserialize(&payload[4..])?;
        handler(resp);
    }
    Ok(())
}
