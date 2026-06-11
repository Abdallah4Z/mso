use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::Duration;

fn start_daemon(temp_dir: &std::path::Path, port: u16) -> (Child, PathBuf) {
    let mso_dir = temp_dir.join(".mso");
    let sock_path = mso_dir.join("mso.sock");
    let log_path = temp_dir.join("daemon.log");
    let mso_bin = PathBuf::from(env!("CARGO_BIN_EXE_mso"));

    let mut cmd = Command::new(&mso_bin);
    cmd.arg("daemon")
        .env("HOME", temp_dir)
        .env("MSO_METRICS_PORT", port.to_string())
        .stdout(std::fs::File::create(&log_path).unwrap())
        .stderr(std::fs::File::create(&log_path).unwrap());

    let child = cmd.spawn().expect("failed to start daemon");

    for _ in 0..50 {
        if sock_path.exists() {
            return (child, sock_path);
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    // Log the daemon output for debugging
    if let Ok(log) = std::fs::read_to_string(&log_path) {
        eprintln!("daemon log:\n{}", log);
    }
    panic!("daemon socket did not appear at {:?}", sock_path);
}

fn encode_msg(msg: &mso::protocol::ClientMessage) -> Vec<u8> {
    let payload = bincode::serialize(msg).unwrap();
    let total = 4u32 + payload.len() as u32;
    let mut wire = Vec::with_capacity(4 + 4 + payload.len());
    wire.extend_from_slice(&total.to_le_bytes());
    wire.extend_from_slice(b"MSO\0");
    wire.extend_from_slice(&payload);
    wire
}

fn roundtrip(sock: &PathBuf, msg: &mso::protocol::ClientMessage) -> mso::protocol::DaemonMessage {
    let wire = encode_msg(msg);
    let mut stream = UnixStream::connect(sock).unwrap();
    stream.write_all(&wire).unwrap();

    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).unwrap();
    let len = u32::from_le_bytes(len_buf) as usize;
    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload).unwrap();
    assert!(payload.len() >= 4);
    assert_eq!(&payload[0..4], b"MSO\0");
    bincode::deserialize(&payload[4..]).unwrap()
}

fn setup() -> (Child, PathBuf, tempfile::TempDir) {
    let dir = tempfile::TempDir::new().unwrap();
    let port = 19750 + (std::process::id() % 1000) as u16;
    let (daemon, sock) = start_daemon(dir.path(), port);
    (daemon, sock, dir)
}

#[test]
fn test_ping_pong() {
    let (mut daemon, sock, _dir) = setup();
    let resp = roundtrip(&sock, &mso::protocol::ClientMessage::Ping);
    assert!(matches!(resp, mso::protocol::DaemonMessage::Pong));
    daemon.kill().ok();
}

#[test]
fn test_get_state_empty() {
    let (mut daemon, sock, _dir) = setup();
    let resp = roundtrip(&sock, &mso::protocol::ClientMessage::GetState);
    match resp {
        mso::protocol::DaemonMessage::StateSnapshot(snap) => {
            assert!(snap.processes.is_empty());
        }
        _ => panic!("expected StateSnapshot"),
    }
    daemon.kill().ok();
}

#[test]
fn test_register_and_get_state() {
    let (mut daemon, sock, _dir) = setup();

    let reg = mso::protocol::ProcessRegistration {
        pid: 0,
        command: vec!["echo".into(), "hello".into()],
        working_dir: std::path::PathBuf::from("/tmp"),
        env_vars: std::collections::HashMap::new(),
        silence_duration: Some(0),
        restart_policy: mso::protocol::RestartPolicy::No,
        tags: vec!["test".into()],
        health_check: None,
        log_file: None,
    };
    let resp = roundtrip(&sock, &mso::protocol::ClientMessage::RegisterProcess(reg));
    match resp {
        mso::protocol::DaemonMessage::Registered { pid, .. } => {
            assert!(pid > 0);
        }
        _ => panic!("expected Registered, got {:?}", resp),
    }

    // Now get state
    let resp2 = roundtrip(&sock, &mso::protocol::ClientMessage::GetState);
    match resp2 {
        mso::protocol::DaemonMessage::StateSnapshot(snap) => {
            assert_eq!(snap.processes.len(), 1);
            assert_eq!(snap.processes[0].tags, vec!["test"]);
        }
        _ => panic!("expected StateSnapshot"),
    }

    daemon.kill().ok();
}

#[test]
fn test_prune_logs() {
    let (mut daemon, sock, _dir) = setup();
    let resp = roundtrip(&sock, &mso::protocol::ClientMessage::PruneLogs {
        older_than_days: 1,
        process_id: None,
    });
    match resp {
        mso::protocol::DaemonMessage::LogsPruned { count } => {
            assert_eq!(count, 0);
        }
        _ => panic!("expected LogsPruned, got {:?}", resp),
    }
    daemon.kill().ok();
}
