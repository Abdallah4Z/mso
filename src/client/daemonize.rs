use crate::util;
use anyhow::Context;
use std::time::Duration;
use tokio::net::UnixStream;

/// Ensure the background daemon is running. Spawn it if not.
pub async fn ensure_daemon() -> anyhow::Result<()> {
    let sock_path = util::mso_sock_path();

    // Check existing daemon via pid file + socket
    if util::read_daemon_pid().is_some() && UnixStream::connect(&sock_path).await.is_ok() {
        return Ok(());
    }

    // No daemon running — spawn `mso daemon` as a detached background process
    eprintln!("[mso] starting background daemon...");

    let myself = std::env::current_exe().context("resolving mso binary path")?;

    // Spawn the daemon process detached from the terminal
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let log_path = util::mso_dir().join("daemon.log");
        let log_file = std::fs::OpenOptions::new()
            .create(true).write(true).truncate(true)
            .open(&log_path)
            .context("opening daemon log")?;
        let log_err = log_file.try_clone().context("cloning log fd")?;
        unsafe {
            std::process::Command::new(&myself)
                .arg("daemon")
                .stdout(log_file)
                .stderr(log_err)
                .stdin(std::process::Stdio::null())
                .pre_exec(|| {
                    if libc::setsid() == -1 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                })
                .spawn()
                .context("spawning daemon process")?;
        }
    }

    #[cfg(not(unix))]
    {
        std::process::Command::new(&myself)
            .arg("daemon")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .stdin(std::process::Stdio::null())
            .spawn()
            .context("spawning daemon process")?;
    }

    // Wait for daemon socket to become available (max 5 seconds)
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if UnixStream::connect(&sock_path).await.is_ok() {
            tracing::info!("daemon is ready");
            return Ok(());
        }
    }

    anyhow::bail!("daemon failed to start within 5 seconds")
}

/// Connect to the daemon's UDS socket.
pub async fn connect_daemon() -> anyhow::Result<UnixStream> {
    let sock_path = util::mso_sock_path();
    let stream = UnixStream::connect(&sock_path).await
        .context("failed to connect to daemon — is it running?")?;
    Ok(stream)
}
