use crate::Result;
use crate::cli::BrokerArgs;
use crate::core::operations;
use crate::core::options::BrokerOptions;

pub fn handle_broker(args: BrokerArgs) -> Result<()> {
    let options = BrokerOptions {
        port: args.port,
        pidfile: args.pidfile,
        logfile: args.logfile,
        handshake_dir: args.handshake_dir,
    };
    let _ = operations::broker(options, None)?;
    Ok(())
}
