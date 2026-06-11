pub mod app;
pub mod event;
pub mod ui;
pub mod widgets;
pub mod theme;

use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    event::{EnableMouseCapture, DisableMouseCapture},
};
use ratatui::prelude::*;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;

static READONLY: AtomicBool = AtomicBool::new(false);

pub fn readonly() -> bool {
    READONLY.load(Ordering::SeqCst)
}

pub async fn run() -> anyhow::Result<()> {
    run_with_options(false).await
}

/// Run the TUI with options.
pub async fn run_with_options(readonly: bool) -> anyhow::Result<()> {
    use anyhow::Context;

    READONLY.store(readonly, Ordering::SeqCst);
    crate::client::daemonize::ensure_daemon().await?;

    // Initialize theme
    let cfg = crate::util::Config::load();
    let theme = crate::tui::theme::Theme::from_config(&cfg.theme);
    let colors = theme.colors_with_overrides(&cfg.theme);
    crate::tui::theme::init_with_colors(colors);

    let sock_path = crate::util::mso_sock_path();
    let stream = tokio::net::UnixStream::connect(&sock_path).await
        .context("cannot connect to MSO daemon — is it running?")?;

    run_with_socket_impl(stream).await
}

/// Run the TUI connected to a specific socket (used by remote SSH tunnel).
pub async fn run_with_socket(socket_path: &str) -> anyhow::Result<()> {
    // Initialize theme
    let cfg = crate::util::Config::load();
    let theme = crate::tui::theme::Theme::from_config(&cfg.theme);
    let colors = theme.colors_with_overrides(&cfg.theme);
    crate::tui::theme::init_with_colors(colors);

    let stream = tokio::net::UnixStream::connect(socket_path).await
        .map_err(|e| anyhow::anyhow!("cannot connect to {socket_path}: {e}"))?;
    run_with_socket_impl(stream).await
}

async fn run_with_socket_impl(stream: tokio::net::UnixStream) -> anyhow::Result<()> {

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (event_tx, event_rx) = mpsc::unbounded_channel::<event::TuiEvent>();
    let (write_tx, mut write_rx) = mpsc::unbounded_channel::<Vec<u8>>();

    // Split the stream for concurrent read/write
    let (mut reader, mut writer) = stream.into_split();

    // Spawn crossterm event reader
    let evt_tx = event_tx.clone();
    tokio::spawn(async move {
        let mut crossterm_stream = crossterm::event::EventStream::new();
        use futures::StreamExt;
        loop {
            if let Some(Ok(evt)) = crossterm_stream.next().await {
                match evt {
                    crossterm::event::Event::Key(key) => {
                        let _ = evt_tx.send(event::TuiEvent::Key(key));
                    }
                    crossterm::event::Event::Mouse(me) => {
                        let _ = evt_tx.send(event::TuiEvent::Mouse(me));
                    }
                    crossterm::event::Event::Resize(_, _) => {
                        let _ = evt_tx.send(event::TuiEvent::Tick);
                    }
                    _ => {}
                }
            }
        }
    });

    // Spawn daemon message reader
    let evt_tx2 = event_tx.clone();
    tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        loop {
            let mut len_buf = [0u8; 4];
            match reader.read_exact(&mut len_buf).await {
                Ok(_) => {}
                Err(_) => break,
            }
            let len = u32::from_le_bytes(len_buf) as usize;
            if len > 16 * 1024 * 1024 {
                break;
            }
            let mut payload = vec![0u8; len];
            if reader.read_exact(&mut payload).await.is_err() {
                break;
            }
            if payload.len() < 4 || payload[0..4] != crate::protocol::WIRE_MAGIC {
                continue;
            }
            if let Ok(msg) = bincode::deserialize::<crate::protocol::DaemonMessage>(&payload[4..]) {
                let _ = evt_tx2.send(event::TuiEvent::DaemonMsg(msg));
            }
        }
    });

    // Spawn daemon write pump — drains write_rx channel and writes to the socket
    tokio::spawn(async move {
        use tokio::io::AsyncWriteExt;
        while let Some(wire) = write_rx.recv().await {
            if writer.write_all(&wire).await.is_err() {
                break;
            }
        }
    });

    // Send initial GetState
    {
        let wire = crate::protocol::encode_message(&crate::protocol::ClientMessage::GetState)?;
        let _ = write_tx.send(wire);
    }

    // Spawn tick timer
    let tick_tx = event_tx.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let _ = tick_tx.send(event::TuiEvent::Tick);
        }
    });

    let mut app = app::App::new(event_tx, write_tx);

    let result = run_loop(&mut terminal, &mut app, event_rx).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    result
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut app::App,
    mut event_rx: mpsc::UnboundedReceiver<event::TuiEvent>,
) -> anyhow::Result<()> {
    while let Some(evt) = event_rx.recv().await {
        match evt {
            event::TuiEvent::Key(key) => {
                app.handle_key(key);
                if app.should_quit {
                    break;
                }
            }
            event::TuiEvent::Mouse(me) => {
                app.handle_mouse(me);
            }
            event::TuiEvent::DaemonMsg(msg) => {
                app.on_daemon_msg(msg).await;
            }
            event::TuiEvent::Tick => {
                app.on_tick();
            }
        }

        terminal.draw(|f| {
            ui::render(app, f);
        })?;
    }
    Ok(())
}
