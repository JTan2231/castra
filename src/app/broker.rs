use std::fs;
use std::io::{self, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::cli::BrokerArgs;
use crate::error::{CliError, CliResult};
use libc;

pub fn handle_broker(args: BrokerArgs) -> CliResult<()> {
    run_broker(args)
}

fn run_broker(args: BrokerArgs) -> CliResult<()> {
    if let Some(parent) = args.pidfile.parent() {
        fs::create_dir_all(parent).map_err(|err| CliError::PreflightFailed {
            message: format!(
                "Failed to prepare broker pidfile directory {}: {err}",
                parent.display()
            ),
        })?;
    }
    if let Some(parent) = args.logfile.parent() {
        fs::create_dir_all(parent).map_err(|err| CliError::PreflightFailed {
            message: format!(
                "Failed to prepare broker log directory {}: {err}",
                parent.display()
            ),
        })?;
    }

    let listener =
        TcpListener::bind(("127.0.0.1", args.port)).map_err(|err| CliError::PreflightFailed {
            message: format!("Broker failed to bind 127.0.0.1:{}: {err}", args.port),
        })?;

    let mut log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&args.logfile)
        .map_err(|err| CliError::PreflightFailed {
            message: format!(
                "Unable to open broker log {}: {err}",
                args.logfile.display()
            ),
        })?;

    fs::write(&args.pidfile, format!("{}\n", std::process::id())).map_err(|err| {
        CliError::PreflightFailed {
            message: format!(
                "Failed to write broker pidfile {}: {err}",
                args.pidfile.display()
            ),
        }
    })?;
    let _guard = PidfileGuard {
        path: args.pidfile.clone(),
    };

    broker_log_line(
        &mut log,
        "INFO",
        &format!("listening on 127.0.0.1:{}", args.port),
    )?;

    loop {
        match listener.accept() {
            Ok((mut stream, addr)) => {
                broker_log_line(&mut log, "INFO", &format!("connection from {addr}"))?;
                let _ = stream.write_all(b"castra-broker 0.1 ready\n");
            }
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => {
                broker_log_line(&mut log, "ERROR", &format!("accept failed: {err}"))?;
                thread::sleep(Duration::from_millis(200));
            }
        }
    }
}

struct PidfileGuard {
    path: PathBuf,
}

impl Drop for PidfileGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn broker_log_line(log: &mut fs::File, level: &str, message: &str) -> CliResult<()> {
    let line = format!("[host-broker] {} {} {}", broker_timestamp(), level, message);
    log.write_all(line.as_bytes())
        .map_err(|err| CliError::PreflightFailed {
            message: format!("Failed to write broker log entry: {err}"),
        })?;
    log.write_all(b"\n")
        .map_err(|err| CliError::PreflightFailed {
            message: format!("Failed to finalize broker log entry: {err}"),
        })?;
    log.flush().map_err(|err| CliError::PreflightFailed {
        message: format!("Failed to flush broker log: {err}"),
    })?;
    Ok(())
}

fn broker_timestamp() -> String {
    let now = SystemTime::now();
    let Ok(duration) = now.duration_since(UNIX_EPOCH) else {
        return "00:00:00".to_string();
    };
    let mut raw: libc::time_t = duration.as_secs() as libc::time_t;
    let mut tm = unsafe { std::mem::zeroed::<libc::tm>() };
    let ptr = unsafe { libc::localtime_r(&mut raw, &mut tm) };
    if ptr.is_null() {
        return "00:00:00".to_string();
    }
    format!("{:02}:{:02}:{:02}", tm.tm_hour, tm.tm_min, tm.tm_sec)
}
