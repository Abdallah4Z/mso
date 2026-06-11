use crate::client::daemonize;
use crate::protocol::*;
use anyhow::Context;
use std::collections::HashMap;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Run a child process under MSO supervision.
///
/// `silence_secs`: `Some(0)` = immediate background, `Some(n)` = live stream for n seconds,
///                 `None` = stream forever (no auto-background).
pub async fn run_wrapped(command: Vec<String>, silence_secs: Option<u64>, restart_policy: RestartPolicy, tags: Vec<String>, health_check: Option<HealthCheckConfig>, log_file: Option<String>) -> anyhow::Result<()> {
    daemonize::ensure_daemon().await?;

    let working_dir = std::env::current_dir().context("getting cwd")?;
    let env_vars: HashMap<String, String> = std::env::vars().collect();

    // Connect to daemon and register — daemon spawns the process
    let mut stream = daemonize::connect_daemon().await?;
    let reg = ProcessRegistration {
        pid: 0,
        command: command.clone(),
        working_dir,
        env_vars,
        silence_duration: silence_secs,
        restart_policy,
        tags,
        health_check,
        log_file,
    };
    let wire = encode_message(&ClientMessage::RegisterProcess(reg))?;
    stream.write_all(&wire).await?;

    // Read Registered response
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let resp_len = u32::from_le_bytes(len_buf) as usize;
    let mut resp_payload = vec![0u8; resp_len];
    stream.read_exact(&mut resp_payload).await?;
    let resp: DaemonMessage = if resp_payload.len() >= 4 && resp_payload[0..4] == WIRE_MAGIC {
        bincode::deserialize(&resp_payload[4..])?
    } else {
        bincode::deserialize(&resp_payload)?
    };

    let (process_id, pid) = match resp {
        DaemonMessage::Registered { id, pid } => (id, pid),
        DaemonMessage::Error(e) => anyhow::bail!("daemon rejected: {e}"),
        _ => anyhow::bail!("unexpected daemon response"),
    };

    eprintln!("[mso] managed PID {} (id: {})", pid, &process_id.to_string()[..8]);

    // If immediate background, just print the PID and exit
    if silence_secs == Some(0) {
        eprintln!("[mso] backgrounded immediately");
        return Ok(());
    }

    // Subscribe to live log stream
    let stream_wire = encode_message(&ClientMessage::StreamLogs(process_id))?;
    stream.write_all(&stream_wire).await?;

    // Determine how long to stream
    let deadline = silence_secs.map(|n| tokio::time::Instant::now() + Duration::from_secs(n));

    if let Some(n) = silence_secs {
        eprintln!("[mso] streaming output for {n}s...");
    } else {
        eprintln!("[mso] streaming output (Ctrl+C to stop)...");
    }

    // Read log events from daemon and print to terminal
    loop {
        tokio::select! {
            result = read_daemon_message(&mut stream) => {
                match result {
                    Ok(msg) => {
                        match msg {
                            DaemonMessage::LogEvent { line, stream: StreamKind::Stdout, .. } => {
                                use std::io::Write;
                                print!("{line}");
                                let _ = std::io::stdout().flush();
                            }
                            DaemonMessage::LogEvent { line, stream: StreamKind::Stderr, .. } => {
                                use std::io::Write;
                                eprint!("{line}");
                                let _ = std::io::stderr().flush();
                            }
                            DaemonMessage::StatusChanged { status: ProcessStatus::Stopped, .. } => {
                                eprintln!("\n[mso] process exited");
                                break;
                            }
                            DaemonMessage::StatusChanged { status: ProcessStatus::Crashed, .. } => {
                                eprintln!("\n[mso] process crashed");
                                break;
                            }
                            _ => {}
                        }
                    }
                    Err(e) => {
                        eprintln!("\n[mso] lost connection to daemon: {e}");
                        break;
                    }
                }
            }
            _ = sleep_until_option(deadline) => {
                eprintln!("\n[mso] process backgrounded successfully (PID {})", pid);
                break;
            }
        }
    }

    Ok(())
}

async fn read_daemon_message(stream: &mut tokio::net::UnixStream) -> anyhow::Result<DaemonMessage> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > 16 * 1024 * 1024 {
        anyhow::bail!("message too large: {len} bytes");
    }
    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload).await?;
    if payload.len() < 4 || payload[0..4] != WIRE_MAGIC {
        anyhow::bail!("wire magic mismatch");
    }
    let msg: DaemonMessage = bincode::deserialize(&payload[4..])?;
    Ok(msg)
}

async fn sleep_until_option(deadline: Option<tokio::time::Instant>) {
    match deadline {
        Some(dl) => tokio::time::sleep_until(dl).await,
        None => std::future::pending::<()>().await,
    }
}
