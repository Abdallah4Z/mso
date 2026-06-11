use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use uuid::Uuid;

pub const WIRE_MAGIC: [u8; 4] = *b"MSO\0";
pub const MAX_LOG_LINES: usize = 500;

// ── Log lines ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimestampedLine {
    pub timestamp: u64,
    pub line: String,
}

// ── Client → Daemon ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    RegisterProcess(ProcessRegistration),
    GetState,
    GetLogs {
        process_id: Uuid,
        offset: usize,
        limit: usize,
    },
    SearchLogs {
        process_id: Uuid,
        query: String,
        stream: Option<StreamKind>,
        offset: usize,
        limit: usize,
    },
    StreamLogs(Uuid),
    StopStream(Uuid),
    LogLine {
        process_id: Uuid,
        stream: StreamKind,
        line: String,
    },
    ProcessExited {
        process_id: Uuid,
        exit_code: Option<i32>,
    },
    RestartProcess(Uuid),
    KillProcess(Uuid),
    PruneLogs {
        older_than_days: u64,
        process_id: Option<Uuid>,
    },
    Ping,
}

// ── Daemon → Client ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DaemonMessage {
    Registered { id: Uuid, pid: u32 },
    StateSnapshot(Box<ProcessSnapshot>),
    LogEvent {
        process_id: Uuid,
        stream: StreamKind,
        line: String,
        timestamp: u64,
    },
    LogsBatch {
        logs: Vec<TimestampedLine>,
        total: usize,
    },
    SearchResult {
        logs: Vec<TimestampedLine>,
        total: usize,
        query: String,
    },
    StatusChanged {
        process_id: Uuid,
        status: ProcessStatus,
    },
    HealthStatus {
        process_id: Uuid,
        healthy: bool,
        failures: u32,
    },
    TelemetryUpdate {
        process_id: Uuid,
        telemetry: ProcessTelemetry,
    },
    LogsPruned {
        count: usize,
    },
    Pong,
    Error(String),
}

// ── Shared types ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StreamKind {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum RestartPolicy {
    #[default]
    No,
    Always,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessRegistration {
    pub pid: u32,
    pub command: Vec<String>,
    pub working_dir: PathBuf,
    pub env_vars: HashMap<String, String>,
    pub silence_duration: Option<u64>,
    pub restart_policy: RestartPolicy,
    pub tags: Vec<String>,
    pub health_check: Option<HealthCheckConfig>,
    pub log_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessSnapshot {
    pub processes: Vec<ManagedProcess>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedProcess {
    pub id: Uuid,
    pub pid: u32,
    pub command: Vec<String>,
    pub working_dir: PathBuf,
    pub env_vars: HashMap<String, String>,
    pub status: ProcessStatus,
    pub started_at: u64,
    pub uptime_secs: u64,
    pub ports: Vec<u16>,
    pub cpu_percent: f32,
    pub memory_bytes: u64,
    pub io_bytes: u64,
    pub restart_policy: RestartPolicy,
    pub tags: Vec<String>,
    pub health_check: Option<HealthCheckConfig>,
    pub log_file: Option<String>,
    pub health_ok: bool,
    pub sparkline_cpu: String,
    pub sparkline_mem: String,
    pub log_lines: VecDeque<TimestampedLine>,
}

impl ManagedProcess {
    pub fn from_registration(id: Uuid, pid: u32, reg: &ProcessRegistration) -> Self {
        Self {
            id, pid,
            command: reg.command.clone(),
            working_dir: reg.working_dir.clone(),
            env_vars: reg.env_vars.clone(),
            status: ProcessStatus::Running,
            started_at: crate::util::epoch_millis(),
            restart_policy: reg.restart_policy,
            tags: reg.tags.clone(),
            health_check: reg.health_check.clone(),
            log_file: reg.log_file.clone(),
            health_ok: true,
            ..Default::default()
        }
    }
}

impl Default for ManagedProcess {
    fn default() -> Self {
        Self {
            id: Uuid::nil(),
            pid: 0,
            command: Vec::new(),
            working_dir: PathBuf::new(),
            env_vars: HashMap::new(),
            status: ProcessStatus::Running,
            started_at: 0,
            uptime_secs: 0,
            ports: Vec::new(),
            cpu_percent: 0.0,
            memory_bytes: 0,
            io_bytes: 0,
            restart_policy: RestartPolicy::No,
            tags: Vec::new(),
            health_check: None,
            log_file: None,
            health_ok: true,
            sparkline_cpu: String::new(),
            sparkline_mem: String::new(),
            log_lines: VecDeque::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessStatus {
    Running,
    Sleeping,
    Crashed,
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessTelemetry {
    pub ports: Vec<u16>,
    pub cpu_percent: f32,
    pub memory_bytes: u64,
    pub io_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    pub url: String,
    pub interval_secs: u64,
    pub timeout_secs: u64,
    pub max_failures: u32,
}

// ── Wire framing helpers ───────────────────────────────────────────

pub fn encode_message<T: Serialize>(msg: &T) -> anyhow::Result<Vec<u8>> {
    let payload = bincode::serialize(msg)?;
    let total_after_len = (4 + payload.len()) as u32;
    let len_bytes = total_after_len.to_le_bytes();
    let mut wire = Vec::with_capacity(4 + 4 + payload.len());
    wire.extend_from_slice(&len_bytes);
    wire.extend_from_slice(&WIRE_MAGIC);
    wire.extend_from_slice(&payload);
    Ok(wire)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_roundtrip() {
        let msg = ClientMessage::Ping;
        let wire = encode_message(&msg).unwrap();
        let payload = wire[4..].to_vec();
        let decoded: ClientMessage = bincode::deserialize(&payload[4..]).unwrap();
        assert!(matches!(decoded, ClientMessage::Ping));
    }

    #[test]
    fn test_register_process_roundtrip() {
        let reg = ProcessRegistration {
            pid: 12345,
            command: vec!["bash".into(), "-c".into(), "echo hi".into()],
            working_dir: PathBuf::from("/tmp"),
            env_vars: [("PATH".into(), "/usr/bin".into())].into(),
            silence_duration: Some(5),
            restart_policy: RestartPolicy::Always,
            tags: Vec::new(),
            health_check: None,
            log_file: None,
        };
        let msg = ClientMessage::RegisterProcess(reg.clone());
        let wire = encode_message(&msg).unwrap();
        let payload = wire[4..].to_vec();
        let decoded: ClientMessage = bincode::deserialize(&payload[4..]).unwrap();
        match decoded {
            ClientMessage::RegisterProcess(d) => {
                assert_eq!(d.pid, 12345);
                assert_eq!(d.command, vec!["bash", "-c", "echo hi"]);
                assert_eq!(d.restart_policy, RestartPolicy::Always);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_timestamped_line_roundtrip() {
        let tl = TimestampedLine { timestamp: 987654321, line: "hello world".into() };
        let wire = encode_message(&tl).unwrap();
        let payload = wire[4..].to_vec();
        let decoded: TimestampedLine = bincode::deserialize(&payload[4..]).unwrap();
        assert_eq!(decoded.timestamp, 987654321);
        assert_eq!(decoded.line, "hello world");
    }

    #[test]
    fn test_wire_magic() {
        let msg = DaemonMessage::Pong;
        let wire = encode_message(&msg).unwrap();
        assert_eq!(wire[4..8], WIRE_MAGIC);
    }
}
