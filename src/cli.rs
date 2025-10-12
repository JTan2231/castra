use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

const VERSION: &str = env!("CASTRA_VERSION");

/// Top-level CLI definition for the `castra` tool.
#[derive(Debug, Parser)]
#[command(
    name = "castra",
    author = "Castra Project",
    version = VERSION,
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
    #[arg(long, help = "Skip config discovery. Requires --config PATH when set.")]
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
    #[arg(long, help = "Skip config discovery. Requires --config PATH when set.")]
    pub skip_discovery: bool,
}

#[derive(Debug, Args, Default)]
pub struct StatusArgs {
    /// Only use the explicit --config path instead of searching parent directories.
    #[arg(long, help = "Skip config discovery. Requires --config PATH when set.")]
    pub skip_discovery: bool,
}

#[derive(Debug, Args, Default)]
pub struct PortsArgs {
    /// Only use the explicit --config path instead of searching parent directories.
    #[arg(long, help = "Skip config discovery. Requires --config PATH when set.")]
    pub skip_discovery: bool,

    /// Verbose output including planned but inactive forwards.
    #[arg(
        long,
        help = "Display verbose forward information, even for inactive VMs"
    )]
    pub verbose: bool,
}

#[derive(Debug, Args, Default)]
pub struct LogsArgs {
    /// Only use the explicit --config path instead of searching parent directories.
    #[arg(long, help = "Skip config discovery. Requires --config PATH when set.")]
    pub skip_discovery: bool,

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

    #[arg(long, value_name = "PATH")]
    pub handshake_dir: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, error::ErrorKind};
    use std::path::Path;

    #[test]
    fn parse_init_defaults() {
        let cli = Cli::try_parse_from(["castra", "init"]).expect("parse init");
        let Commands::Init(args) = cli.command.expect("init command present") else {
            panic!("expected init command");
        };
        assert!(!args.force);
        assert!(args.project_name.is_none());
        assert!(args.output.is_none());
        assert!(cli.config.is_none());
    }

    #[test]
    fn parse_init_with_flags() {
        let cli = Cli::try_parse_from([
            "castra",
            "--config",
            "/tmp/castra.toml",
            "init",
            "--project-name",
            "demo",
            "--output",
            "foo.toml",
            "--force",
        ])
        .expect("parse init with flags");
        assert_eq!(
            cli.config.as_deref(),
            Some(PathBuf::from("/tmp/castra.toml").as_path())
        );
        let Commands::Init(args) = cli.command.expect("init command present") else {
            panic!("expected init command");
        };
        assert!(args.force);
        assert_eq!(args.project_name.as_deref(), Some("demo"));
        assert_eq!(args.output.as_deref(), Some(Path::new("foo.toml")));
    }

    #[test]
    fn parse_up_flags() {
        let cli =
            Cli::try_parse_from(["castra", "up", "--skip-discovery", "--force"]).expect("parse up");
        let Commands::Up(args) = cli.command.expect("up command present") else {
            panic!("expected up command");
        };
        assert!(args.skip_discovery);
        assert!(args.force);
    }

    #[test]
    fn parse_logs_tail_defaults() {
        let cli = Cli::try_parse_from(["castra", "logs", "--tail", "50"]).expect("parse logs tail");
        let Commands::Logs(args) = cli.command.expect("logs command present") else {
            panic!("expected logs command");
        };
        assert_eq!(args.tail, 50);
        assert!(!args.follow);
        assert!(!args.skip_discovery);
    }

    #[test]
    fn parse_hidden_broker_command() {
        let cli = Cli::try_parse_from([
            "castra",
            "broker",
            "--port",
            "8080",
            "--pidfile",
            "/tmp/broker.pid",
            "--logfile",
            "/tmp/broker.log",
            "--handshake-dir",
            "/tmp/handshakes",
        ])
        .expect("parse broker");
        let Commands::Broker(args) = cli.command.expect("broker command present") else {
            panic!("expected broker command");
        };
        assert_eq!(args.port, 8080);
        assert_eq!(args.pidfile, PathBuf::from("/tmp/broker.pid"));
        assert_eq!(args.logfile, PathBuf::from("/tmp/broker.log"));
        assert_eq!(args.handshake_dir, PathBuf::from("/tmp/handshakes"));
    }

    #[test]
    fn logs_tail_requires_number() {
        let err = Cli::try_parse_from(["castra", "logs", "--tail", "abc"]).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::ValueValidation);
    }

    #[test]
    fn parse_logs_skip_discovery_flag() {
        let cli = Cli::try_parse_from([
            "castra",
            "--config",
            "/tmp/castra.toml",
            "logs",
            "--skip-discovery",
        ])
        .expect("parse logs skip discovery");
        let Commands::Logs(args) = cli.command.expect("logs command present") else {
            panic!("expected logs command");
        };
        assert!(args.skip_discovery);
    }

    #[test]
    fn parse_ports_skip_discovery_flag() {
        let cli = Cli::try_parse_from([
            "castra",
            "--config",
            "/tmp/castra.toml",
            "ports",
            "--skip-discovery",
            "--verbose",
        ])
        .expect("parse ports flags");
        let Commands::Ports(args) = cli.command.expect("ports command present") else {
            panic!("expected ports command");
        };
        assert!(args.skip_discovery);
        assert!(args.verbose);
    }

    #[test]
    fn command_reports_embedded_version_string() {
        let command = Cli::command();
        assert_eq!(command.get_version(), Some(env!("CASTRA_VERSION")));
    }
}
