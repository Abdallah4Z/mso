# Wire Protocol

MSO uses a simple length-prefixed binary protocol over Unix domain sockets. Messages are serialized with **bincode**.

## Frame Format

```
┌──────────────────────────────────────────────────────────────┐
│ Byte offset │ Size │ Field            │ Description          │
├───────────────┼──────┼──────────────────┼──────────────────────┤
│ 0            │ 4    │ Length           │ Total bytes after    │
│              │      │                  │ this field. LE u32.  │
│              │      │                  │ = 4 + payload.len() │
├───────────────┼──────┼──────────────────┼──────────────────────┤
│ 4            │ 4    │ Magic            │ b"MSO\0"             │
├───────────────┼──────┼──────────────────┼──────────────────────┤
│ 8            │ N    │ Payload          │ Bincode-serialized   │
│              │      │                  │ ClientMessage or     │
│              │      │                  │ DaemonMessage        │
└───────────────┴──────┴──────────────────┴──────────────────────┘
```

**Total wire size:** 8 + N bytes (4 length + 4 magic + N payload).

The length field encodes the number of bytes **after** itself — i.e., `4 + N`. This allows the reader to know exactly how many bytes to read for the magic+payload.

### Example Hex Dump

```
Length:     0D 00 00 00    (13 = 4 magic + 9 payload)
Magic:      4D 53 4F 00    ("MSO\0")
Payload:    ... bincode ...
```

## Protocol Version

Current version: **3**

The version constant is defined in `protocol.rs`:
```rust
pub const PROTOCOL_VERSION: u32 = 3;
```

## Client → Daemon Messages

All messages are variants of the `ClientMessage` enum.

### `RegisterProcess(ProcessRegistration)`

Register a new process with the daemon. If `pid` is 0, the daemon spawns the process itself.

```rust
pub struct ProcessRegistration {
    pub pid: u32,                // 0 = daemon spawns, >0 = client spawned
    pub command: Vec<String>,    // Command and arguments
    pub working_dir: PathBuf,    // Working directory
    pub env_vars: HashMap<String, String>,  // Environment variables
    pub silence_duration: Option<u64>,      // Seconds before backgrounding (None = forever)
    pub restart_policy: RestartPolicy,       // No, Always
    pub tags: Vec<String>,                   // Tags for filtering
    pub health_check: Option<HealthCheckConfig>,  // Health check config
    pub alert_webhook: Option<String>,       // Webhook URL for alerts
}
```

**Response:** `Registered { id: Uuid, pid: u32 }`

### `GetState`

Request a snapshot of all managed processes.

**Response:** `StateSnapshot(ProcessSnapshot)`

```rust
pub struct ProcessSnapshot {
    pub processes: Vec<ManagedProcess>,
}
```

### `GetLogs { process_id, offset, limit }`

Get paginated log lines for a process. Offset is from newest (0 = most recent). Results are returned in chronological order.

**Response:** `LogsBatch { logs: Vec<TimestampedLine>, total: usize }`

### `SearchLogs { process_id, query, stream, offset, limit }`

Search logs by keyword. Optional stream filter (Stdout/Stderr).

**Response:** `SearchResult { logs: Vec<TimestampedLine>, total: usize, query: String }`

### `StreamLogs(Uuid)`

Subscribe to live log events for a process. After sending this message, the daemon sends `LogEvent` messages on the same connection.

**Response:** None (fire-and-forget). The daemon starts sending `LogEvent` messages.

### `StopStream(Uuid)`

Unsubscribe from live log events.

**Response:** None (fire-and-forget).

### `LogLine { process_id, stream, line }`

Send a log line from a client-spawned process (legacy).

**Response:** None (fire-and-forget).

### `ProcessExited { process_id, exit_code }`

Notify daemon that a client-spawned process has exited.

**Response:** None (fire-and-forget).

### `RestartProcess(Uuid)`

Restart a managed process (kills old, spawns new).

**Response:** `StatusChanged { process_id: Uuid, status: ProcessStatus }`

### `KillProcess(Uuid)`

Kill a managed process (SIGTERM → 2s → SIGKILL).

**Response:** `StatusChanged { process_id: Uuid, status: ProcessStatus }`

### `PruneLogs { older_than_days, process_id }`

Delete old log entries. Optionally target a specific process.

**Response:** `LogsPruned { count: usize }`

### `Ping`

Health check for the daemon connection.

**Response:** `Pong`

## Daemon → Client Messages

### `Registered { id, pid }`

Response to `RegisterProcess`. Contains the assigned UUID and the actual PID.

### `StateSnapshot(ProcessSnapshot)`

Response to `GetState`. Contains all managed processes with their current metrics and log lines.

### `LogEvent { process_id, stream, line, timestamp }`

Sent in response to `StreamLogs`. Contains a single log line from a running process.

### `LogsBatch { logs, total }`

Response to `GetLogs`. Contains a page of log lines and the total count.

### `SearchResult { logs, total, query }`

Response to `SearchLogs`. Contains matching log lines.

### `StatusChanged { process_id, status }`

Sent when a process status changes (started, stopped, crashed, restarted).

### `TelemetryUpdate { process_id, telemetry }`

Periodic updates with CPU%, memory, ports, and I/O metrics (every 2 seconds).

### `LogsPruned { count }`

Response to `PruneLogs`. Number of log entries deleted.

### `HealthStatus { process_id, healthy, failures }`

Sent when a health check result changes.

### `Pong`

Response to `Ping`.

### `Error(String)`

Generic error response for any request that failed.

## Shared Types

```rust
pub struct TimestampedLine {
    pub timestamp: u64,  // Epoch milliseconds
    pub line: String,
}

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
    pub network_bytes: u64,
    pub restart_policy: RestartPolicy,
    pub tags: Vec<String>,
    pub health_check: Option<HealthCheckConfig>,
    pub alert_webhook: Option<String>,
    pub health_ok: bool,
    pub sparkline_cpu: String,
    pub sparkline_mem: String,
    pub log_lines: VecDeque<TimestampedLine>,
}

pub enum ProcessStatus { Running, Sleeping, Crashed, Stopped }
pub enum StreamKind { Stdout, Stderr }
pub enum RestartPolicy { No, Always }

pub struct HealthCheckConfig {
    pub url: String,
    pub interval_secs: u64,
    pub timeout_secs: u64,
    pub max_failures: u32,
}
```
