use crate::protocol::*;
use crate::daemon::supervisor::ProcessSupervisor;
use crate::daemon::persist;
use crate::daemon::LogBroadcast;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc;
use std::sync::Arc;
use uuid::Uuid;

pub async fn accept_loop(listener: UnixListener, supervisor: Arc<ProcessSupervisor>, log_tx: LogBroadcast) -> anyhow::Result<()> {
    loop {
        let (stream, _addr) = listener.accept().await?;
        let supervisor = supervisor.clone();
        let log_tx = log_tx.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, supervisor, log_tx).await {
                tracing::error!("connection error: {e}");
            }
        });
    }
}

async fn handle_connection(stream: UnixStream, supervisor: Arc<ProcessSupervisor>, log_tx: LogBroadcast) -> anyhow::Result<()> {
    let (mut reader, mut writer) = stream.into_split();
    let (write_tx, mut write_rx) = mpsc::unbounded_channel::<Vec<u8>>();

    // Write pump — drains write_rx and writes to the client socket
    tokio::spawn(async move {
        while let Some(wire) = write_rx.recv().await {
            if writer.write_all(&wire).await.is_err() {
                break;
            }
        }
    });

    loop {
        let mut len_buf = [0u8; 4];
        match reader.read_exact(&mut len_buf).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(e) => return Err(e.into()),
        }

        let len = u32::from_le_bytes(len_buf) as usize;
        if len > 16 * 1024 * 1024 {
            anyhow::bail!("message too large: {len} bytes");
        }

        let mut payload = vec![0u8; len];
        reader.read_exact(&mut payload).await?;

        if payload.len() < 4 || payload[0..4] != WIRE_MAGIC {
            anyhow::bail!("wire magic mismatch");
        }
        let msg: ClientMessage = bincode::deserialize(&payload[4..])?;

        // Fire-and-forget messages: process without sending a response
        match &msg {
            ClientMessage::LogLine { process_id, stream: s, line } => {
                let ts = crate::util::epoch_millis();
                if let Some(mut proc) = supervisor.table.inner.get_mut(process_id) {
                    if proc.log_lines.len() >= MAX_LOG_LINES {
                        proc.log_lines.pop_front();
                    }
                    proc.log_lines.push_back(crate::protocol::TimestampedLine { timestamp: ts, line: line.clone() });
                }
                let _ = supervisor.log_tx.send((*process_id, *s, line.clone(), ts));
                continue;
            }
            ClientMessage::ProcessExited { process_id, exit_code } => {
                if let Some(mut proc) = supervisor.table.inner.get_mut(process_id) {
                    proc.status = if *exit_code == Some(0) {
                        ProcessStatus::Stopped
                    } else {
                        ProcessStatus::Crashed
                    };
                }
                let procs: Vec<_> = supervisor.table.inner.iter().map(|e| e.value().clone()).collect();
                persist::save(&procs);
                tracing::info!(process_id = %process_id, exit_code = ?exit_code, "process exited");
                continue;
            }
            _ => {}
        }

        // Request/response messages
        let response = match msg {
            ClientMessage::RegisterProcess(reg) => {
                let id = Uuid::new_v4();

                // If pid == 0, daemon spawns the process
                let pid = if reg.pid == 0 {
                    match supervisor.spawn(id, &reg).await {
                        Ok(pid) => pid,
                        Err(e) => {
                            return send_response(&write_tx, &DaemonMessage::Error(format!("spawn failed: {e}"))).await;
                        }
                    }
                } else {
                    // Client already spawned (legacy mode)
                    let proc = crate::protocol::ManagedProcess::from_registration(id, reg.pid, &reg);
                    supervisor.table.inner.insert(id, proc);
                    reg.pid
                };

                let procs: Vec<_> = supervisor.table.inner.iter().map(|e| e.value().clone()).collect();
                persist::save(&procs);

                tracing::info!(process_id = %id, pid, "registered new process");
                DaemonMessage::Registered { id, pid }
            }

            ClientMessage::GetState => {
                DaemonMessage::StateSnapshot(Box::new(supervisor.table.snapshot()))
            }

            ClientMessage::GetLogs { process_id, offset, limit } => {
                match supervisor.log_db.get_logs(process_id, offset, limit) {
                    Ok((logs, total)) => DaemonMessage::LogsBatch { logs, total },
                    Err(e) => DaemonMessage::Error(format!("db error: {e}")),
                }
            }

            ClientMessage::SearchLogs { process_id, query, stream, offset, limit } => {
                match supervisor.log_db.search_logs(process_id, &query, stream, offset, limit) {
                    Ok((logs, total)) => DaemonMessage::SearchResult { logs, total, query },
                    Err(e) => DaemonMessage::Error(format!("search error: {e}")),
                }
            }

            ClientMessage::StreamLogs(process_id) => {
                // Spawn a forwarder that streams log events to this client
                let mut log_rx = log_tx.subscribe();
                let fwd_tx = write_tx.clone();
                tokio::spawn(async move {
                    loop {
                        match log_rx.recv().await {
                            Ok((id, stream, line, ts)) => {
                                if id == process_id {
                                    let msg = DaemonMessage::LogEvent { process_id: id, stream, line, timestamp: ts };
                                    if let Ok(wire) = encode_message(&msg) {
                                        if fwd_tx.send(wire).is_err() {
                                            break;
                                        }
                                    }
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                            Err(_) => break,
                        }
                    }
                });
                // Don't send a response — the client will just read LogEvent messages
                continue;
            }

            ClientMessage::StopStream(_process_id) => {
                // Client wants to stop streaming — just continue reading
                // The forwarder will stop when the write channel is dropped
                continue;
            }

            ClientMessage::RestartProcess(id) => {
                match supervisor.restart(id).await {
                    Ok(()) => {
                        let new_id = supervisor.table.inner.iter()
                            .filter(|e| e.value().status == ProcessStatus::Running)
                            .max_by_key(|e| e.value().started_at)
                            .map(|e| *e.key());
                        if let Some(new_id) = new_id {
                            let procs: Vec<_> = supervisor.table.inner.iter().map(|e| e.value().clone()).collect();
                            persist::save(&procs);
                            DaemonMessage::StatusChanged { process_id: new_id, status: ProcessStatus::Running }
                        } else {
                            DaemonMessage::Error("restart succeeded but no running process found".into())
                        }
                    }
                    Err(e) => DaemonMessage::Error(format!("restart failed: {e}")),
                }
            }

            ClientMessage::KillProcess(id) => {
                match supervisor.kill(id).await {
                    Ok(()) => {
                        let procs: Vec<_> = supervisor.table.inner.iter().map(|e| e.value().clone()).collect();
                        persist::save(&procs);
                        DaemonMessage::StatusChanged { process_id: id, status: ProcessStatus::Stopped }
                    }
                    Err(e) => DaemonMessage::Error(format!("kill failed: {e}")),
                }
            }

            ClientMessage::PruneLogs { older_than_days, process_id } => {
                let cutoff = crate::util::epoch_millis() - (older_than_days * 86400 * 1000);
                match supervisor.log_db.prune_before(cutoff, process_id) {
                    Ok(count) => DaemonMessage::LogsPruned { count },
                    Err(e) => DaemonMessage::Error(format!("prune failed: {e}")),
                }
            }

            ClientMessage::Ping => DaemonMessage::Pong,

            // These are handled as fire-and-forget above
            ClientMessage::LogLine { .. } | ClientMessage::ProcessExited { .. } => {
                tracing::warn!("unexpected fire-and-forget message type, skipping");
                continue;
            }
        };

        send_response(&write_tx, &response).await?;
    }
}

async fn send_response(write_tx: &mpsc::UnboundedSender<Vec<u8>>, msg: &DaemonMessage) -> anyhow::Result<()> {
    let wire = encode_message(msg)?;
    write_tx.send(wire).map_err(|_| anyhow::anyhow!("client disconnected"))?;
    Ok(())
}
