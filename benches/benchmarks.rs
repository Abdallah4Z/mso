use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::collections::HashMap;

fn bench_encode_ping(c: &mut Criterion) {
    let msg = mso::protocol::ClientMessage::Ping;
    c.bench_function("encode Ping", |b| {
        b.iter(|| mso::protocol::encode_message(black_box(&msg)));
    });
}

fn bench_decode_pong(c: &mut Criterion) {
    let msg = mso::protocol::DaemonMessage::Pong;
    let wire = mso::protocol::encode_message(&msg).unwrap();
    let payload = wire[4..].to_vec();
    c.bench_function("decode Pong", |b| {
        b.iter(|| {
            let decoded: mso::protocol::DaemonMessage = bincode::deserialize(&payload[4..]).unwrap();
            black_box(decoded);
        });
    });
}

fn bench_register_process_roundtrip(c: &mut Criterion) {
    let reg = mso::protocol::ProcessRegistration {
        pid: 12345,
        command: vec!["bash".into(), "-c".into(), "echo hi".into()],
        working_dir: std::path::PathBuf::from("/tmp"),
        env_vars: [("PATH".into(), "/usr/bin".into())].into(),
        silence_duration: Some(5),
        restart_policy: mso::protocol::RestartPolicy::Always,
        tags: vec!["web".into(), "prod".into()],
        health_check: None,
        log_file: None,
    };
    let msg = mso::protocol::ClientMessage::RegisterProcess(reg);
    c.bench_function("encode RegisterProcess", |b| {
        b.iter(|| mso::protocol::encode_message(black_box(&msg)));
    });
}

fn bench_sqlite_insert(c: &mut Criterion) {
    let db = mso::daemon::log_db::LogDb::open_in_memory().unwrap();
    let pid = uuid::Uuid::new_v4();
    c.bench_function("sqlite insert 100", |b| {
        b.iter(|| {
            for i in 0..100 {
                db.insert(pid, 0, &format!("line {}", i), i * 1000).unwrap();
            }
        });
    });
}

fn bench_sqlite_query(c: &mut Criterion) {
    let db = mso::daemon::log_db::LogDb::open_in_memory().unwrap();
    let pid = uuid::Uuid::new_v4();
    for i in 0..1000 {
        db.insert(pid, 0, &format!("line {}", i), i * 1000).unwrap();
    }
    c.bench_function("sqlite query 100 from 1000", |b| {
        b.iter(|| {
            let (logs, total) = db.get_logs(pid, 0, 100).unwrap();
            black_box((logs, total));
        });
    });
}

fn bench_sparkline_render(c: &mut Criterion) {
    let values: Vec<f32> = (0..60).map(|i| (i as f32 * 1.5) % 100.0).collect();
    c.bench_function("render sparkline 60", |b| {
        b.iter(|| {
            let result: String = values.iter().rev().take(12).map(|v| {
                let bars = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
                let max = 100.0f32;
                let idx = ((v / max) * (bars.len() - 1) as f32).round() as usize;
                bars[idx.min(bars.len() - 1)]
            }).collect();
            black_box(result);
        });
    });
}

criterion_group!(
    benches,
    bench_encode_ping,
    bench_decode_pong,
    bench_register_process_roundtrip,
    bench_sqlite_insert,
    bench_sqlite_query,
    bench_sparkline_render,
);
criterion_main!(benches);
