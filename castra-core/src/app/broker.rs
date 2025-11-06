use crate::Result;
use crate::cli::{self, BrokerArgs};

pub fn handle_broker(_args: BrokerArgs) -> Result<()> {
    cli::print_bus_broker_deprecation();
    Ok(())
}
