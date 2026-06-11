use clap::Parser;
use clap_complete::Shell;

#[derive(Parser)]
#[command(name = "mso", about = "Multi-Stream Orchestrator — process wrapper & dashboard")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(clap::Subcommand)]
pub enum Command {
    /// Run a command under MSO supervision
    Run {
        /// Seconds to stream output live before backgrounding.
        /// Omit the value for immediate detach.
        #[arg(short = 's', value_name = "SECS", num_args = 0..=1, default_missing_value = "0")]
        silence: Option<u64>,

        /// Auto-restart policy: "no" (default) or "always"
        #[arg(long = "restart")]
        restart: Option<String>,

        /// Tags for filtering (can be specified multiple times)
        #[arg(long = "tag")]
        tag: Vec<String>,

        /// Health check URL (e.g. http://localhost:8080/health)
        #[arg(long = "health-check")]
        health_check: Option<String>,

        /// Health check interval in seconds (default: 10)
        #[arg(long = "health-interval", default_value = "10")]
        health_interval: u64,

        /// Health check timeout in seconds (default: 5)
        #[arg(long = "health-timeout", default_value = "5")]
        health_timeout: u64,

        /// Max consecutive failures before restart (default: 3)
        #[arg(long = "health-max-failures", default_value = "3")]
        health_max_failures: u32,

        /// Path to write process stdout/stderr (in addition to daemon capture)
        #[arg(long = "log-file")]
        log_file: Option<String>,

        /// Load preset configuration
        #[arg(long = "preset")]
        preset: Option<String>,

        /// Command and arguments to execute
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },

    /// Open the interactive dashboard
    #[command(alias = "ls")]
    View {
        /// Disable restart/kill actions (monitoring only)
        #[arg(long = "readonly")]
        readonly: bool,
    },

    /// Generate shell completions
    Completion {
        /// Shell type (bash, zsh, fish, powershell, elvish)
        shell: Shell,
    },

    /// Prune old logs from the database
    Prune {
        /// Delete logs older than N days (default: 30)
        #[arg(long = "days", default_value = "30")]
        days: u64,

        /// Only prune logs for a specific process UUID
        #[arg(long = "process")]
        process: Option<String>,
    },

    /// Export logs for a process
    Logs {
        /// Process UUID or PID (partial match)
        id: String,

        /// Output format: text or json
        #[arg(long = "format", default_value = "text")]
        format: String,

        /// Only show the last N lines
        #[arg(long = "tail")]
        tail: Option<usize>,
    },

    /// Run a command directly (no daemon) and exit with its status
    Exec {
        /// Command and arguments to execute
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },


    /// Manage configuration
    Config {
        /// Action: validate, show, path
        action: String,
    },



    /// Show process statistics
    Stats {
        /// Output format: text or json
        #[arg(long = "format", default_value = "text")]
        format: String,
    },




    /// (internal) Run the background daemon
    #[command(hide = true)]
    Daemon,
}
