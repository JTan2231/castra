//! Castra library crate.

/// Core library modules and APIs.
pub mod core;

/// CLI argument parsing and adapters (only when the `cli` feature is enabled).
#[cfg(feature = "cli")]
pub mod cli;

pub mod app;

mod config;
mod error;
mod managed;

pub use config::*;
pub use error::*;
pub use managed::*;
