mod app;
mod cli;

use std::process::ExitCode;

use clap::{CommandFactory, Parser, error::ErrorKind};

use crate::cli::{Cli, Commands};
pub use castra::{Error, Result, core};

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => {
            let _ = err.print();
            return match err.kind() {
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => ExitCode::SUCCESS,
                _ => ExitCode::from(64),
            };
        }
    };

    let Cli { config, command } = cli;

    let command = match command {
        Some(cmd) => cmd,
        None => {
            let mut command = Cli::command();
            let _ = command.print_help();
            println!();
            return ExitCode::from(64);
        }
    };

    let exit = match command {
        Commands::Init(args) => app::handle_init(args, config.as_ref()),
        Commands::Up(args) => app::handle_up(args, config.as_ref()),
        Commands::Down(args) => app::handle_down(args, config.as_ref()),
        Commands::Status(args) => app::handle_status(args, config.as_ref()),
        Commands::Ports(args) => app::handle_ports(args, config.as_ref()),
        Commands::Logs(args) => app::handle_logs(args, config.as_ref()),
        Commands::Clean(args) => app::handle_clean(args, config.as_ref()),
        Commands::Broker(args) => app::handle_broker(args),
    };

    match exit {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("Error: {err}");
            app::error::exit_code(&err)
        }
    }
}
