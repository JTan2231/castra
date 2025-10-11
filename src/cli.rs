use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

/// Top-level CLI definition for the `castra` tool.
#[derive(Debug, Parser)]
#[command(
    name = "castra",
    author = "Castra Project",
    version,
    about = "A user-friendly orchestrator for lightweight QEMU-based sandboxes.",
    long_about = "Castra helps you spin up reproducible, host-friendly QEMU environments.\n\
                  Explore the roadmap in the repo's todo_*.md files for features under active development."
)]
pub struct Cli {
    /// Path to an explicit configuration file. Defaults to searching for `castra.toml`.
    #[arg(
        global = true,
        short,
        long = "config",
        value_name = "PATH",
        help = "Override auto-discovery and load configuration from PATH"
    )]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Scaffold a new castra project with config and working directory.
    Init(InitArgs),
    /// Boot the configured virtual machines.
    Up(UpArgs),
    /// Shut down running virtual machines.
    Down(DownArgs),
    /// Inspect the state of managed virtual machines.
    Status(StatusArgs),
    /// Display declared host/guest forwards and highlight conflicts and broker reservations.
    Ports(PortsArgs),
    /// Tail orchestrator and guest logs.
    Logs(LogsArgs),
    #[command(hide = true)]
    Broker(BrokerArgs),
}

#[derive(Debug, Args)]
pub struct InitArgs {
    /// Overwrite an existing configuration.
    #[arg(
        long,
        help = "Overwrite any existing castra.toml and related workdir artifacts"
    )]
    pub force: bool,

    /// Set the initial project name in the generated configuration.
    #[arg(
        long,
        value_name = "NAME",
        help = "Seed the project configuration with NAME"
    )]
    pub project_name: Option<String>,

    /// Write the configuration to this path instead of ./castra.toml.
    #[arg(
        short,
        long = "output",
        value_name = "PATH",
        help = "Write the generated configuration to PATH"
    )]
    pub output: Option<PathBuf>,
}

#[derive(Debug, Args, Default)]
pub struct UpArgs {
    /// Only use the explicit --config path instead of searching parent directories.
    #[arg(
        long,
        help = "Skip config discovery and require --config for this invocation"
    )]
    pub skip_discovery: bool,

    /// Proceed even if host resource headroom checks fail (use with caution).
    #[arg(
        long,
        help = "Bypass disk/CPU/memory safety checks during preflight (use with caution)"
    )]
    pub force: bool,
}

#[derive(Debug, Args, Default)]
pub struct DownArgs {
    /// Only use the explicit --config path instead of searching parent directories.
    #[arg(
        long,
        help = "Skip config discovery and require --config for this invocation"
    )]
    pub skip_discovery: bool,
}

#[derive(Debug, Args, Default)]
pub struct StatusArgs {
    /// Only use the explicit --config path instead of searching parent directories.
    #[arg(
        long,
        help = "Skip config discovery and require --config for this invocation"
    )]
    pub skip_discovery: bool,
}

#[derive(Debug, Args, Default)]
pub struct PortsArgs {
    /// Verbose output including planned but inactive forwards.
    #[arg(
        long,
        help = "Display verbose forward information, even for inactive VMs"
    )]
    pub verbose: bool,
}

#[derive(Debug, Args, Default)]
pub struct LogsArgs {
    /// Follow logs in real time.
    #[arg(short, long, help = "Stream logs until interrupted")]
    pub follow: bool,

    /// Number of historical lines to display before streaming.
    #[arg(
        long,
        value_name = "LINES",
        default_value = "200",
        help = "Show the most recent LINES before streaming"
    )]
    pub tail: usize,
}

#[derive(Debug, Args)]
#[command(hide = true)]
pub struct BrokerArgs {
    #[arg(long, value_name = "PORT")]
    pub port: u16,

    #[arg(long, value_name = "PATH")]
    pub pidfile: PathBuf,

    #[arg(long, value_name = "PATH")]
    pub logfile: PathBuf,
}
