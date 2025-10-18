//! Castra library crate.

extern crate self as castra;

/// Core library modules and APIs.
pub mod core;

/// CLI argument parsing and adapters (only when the `cli` feature is enabled).
#[cfg(feature = "cli")]
pub mod cli;

/// CLI-facing application helpers (only when the `cli` feature is enabled).
#[cfg(feature = "cli")]
pub mod app;

mod config;
mod error;

pub use config::*;
pub use error::*;
