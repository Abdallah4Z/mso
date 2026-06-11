use crate::daemon::state::ProcessTable;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;

pub async fn serve_metrics(table: ProcessTable, port: u16) {
    let addr = format!("127.0.0.1:{}", port);
    let listener = match TcpListener::bind(&addr) {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("prometheus: failed to bind {addr}: {e}");
            return;
        }
    };
    listener.set_nonblocking(true).ok();
    tracing::info!("prometheus metrics on http://{addr}/metrics");

    let table = Arc::new(table);
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let table = table.clone();
                tokio::task::spawn_blocking(move || {
                    let mut buf = [0u8; 4096];
                    if stream.read(&mut buf).is_err() {
                        return;
                    }
                    let request = String::from_utf8_lossy(&buf);
                    if !request.starts_with("GET /metrics") {
                        let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\n\r\n");
                        return;
                    }

                    let mut body = String::from(
                        "# HELP mso_cpu_percent CPU usage percentage\n# TYPE mso_cpu_percent gauge\n"
                    );
                    body.push_str("# HELP mso_daemon_up Daemon health\n# TYPE mso_daemon_up gauge\nmso_daemon_up 1\n");
                    for entry in table.inner.iter() {
                        let proc = entry.value();
                        let name = proc.command.first().map(|s| s.as_str()).unwrap_or("unknown");
                        let status = format!("{:?}", proc.status).to_lowercase();
                        body.push_str(&format!(
                            "mso_cpu_percent{{pid=\"{}\",name=\"{}\",status=\"{}\"}} {}\n",
                            proc.pid, name, status, proc.cpu_percent
                        ));
                        body.push_str(&format!(
                            "mso_memory_bytes{{pid=\"{}\",name=\"{}\"}} {}\n",
                            proc.pid, name, proc.memory_bytes
                        ));
                        body.push_str(&format!(
                            "mso_io_bytes{{pid=\"{}\",name=\"{}\"}} {}\n",
                            proc.pid, name, proc.io_bytes
                        ));
                        for port in &proc.ports {
                            body.push_str(&format!(
                                "mso_open_port{{pid=\"{}\",port=\"{}\"}} 1\n",
                                proc.pid, port
                            ));
                        }
                        body.push_str(&format!(
                            "mso_uptime_seconds{{pid=\"{}\",name=\"{}\"}} {}\n",
                            proc.pid, name, proc.uptime_secs
                        ));
                    }
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(response.as_bytes());
                });
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Err(e) => {
                tracing::error!("prometheus: accept error: {e}");
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
    }
}
