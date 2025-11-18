//! Server orchestration utilities for the legacy Hotline runtime.
//!
//! This module exposes the command-line interface and reusable helpers that
//! power the current Tokio-based server binary. Binary crates can re-use these
//! entry points to remain thin wrappers that only need to call [`run`].

pub mod cli;
pub mod legacy;
pub mod wireframe;

use anyhow::Result;
use clap::Parser;
pub use cli::{AppConfig, Cli, Commands, CreateUserArgs};
pub use legacy::{run_command, run_daemon};

/// Parse CLI arguments and execute the requested command or daemon.
///
/// # Errors
///
/// Returns any error emitted while parsing configuration or starting the
/// legacy runtime.
pub async fn run() -> Result<()> {
    let cli = Cli::parse();
    run_with_cli(cli).await
}

/// Execute the server logic using an already parsed [`Cli`].
///
/// # Errors
///
/// Propagates any failure reported by [`legacy::dispatch`].
pub async fn run_with_cli(cli: Cli) -> Result<()> { legacy::dispatch(cli).await }
