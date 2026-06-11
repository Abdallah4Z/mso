use mso::cli::{Cli, Command};
use clap::Parser;

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("mso=warn")),
        )
        .with_target(false)
        .init();

    let cli = Cli::parse();

    match cli.command {
        Some(Command::Run { silence, restart, command, tag, health_check, health_interval, health_timeout, health_max_failures, log_file, preset }) =>
            mso::commands::handle_run(mso::commands::RunConfig { silence, restart, command, tags: tag, health_check, health_interval, health_timeout, health_max_failures, log_file, preset }).await,
        Some(Command::View { readonly }) => mso::commands::handle_view(readonly).await,
        None => mso::commands::handle_view_default().await,
        Some(Command::Completion { shell }) => mso::commands::handle_completion(shell),
        Some(Command::Prune { days, process }) => mso::commands::handle_prune(days, process).await,
        Some(Command::Logs { id, format, tail }) => mso::commands::handle_logs(id, format, tail).await,
        Some(Command::Exec { command }) => mso::commands::handle_exec(command),
        Some(Command::Config { action }) => mso::commands::handle_config(action),
        Some(Command::Stats { format }) => mso::commands::handle_stats(format).await,
        Some(Command::Daemon) => mso::commands::handle_daemon().await,
    }
}
