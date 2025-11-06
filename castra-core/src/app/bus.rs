use std::path::PathBuf;

use crate::Result;
use crate::cli::{self, BusArgs};
pub fn handle_bus(_args: BusArgs, _config_override: Option<&PathBuf>) -> Result<()> {
    cli::print_bus_broker_deprecation();
    Ok(())
}
