//! Server orchestration utilities shared by both network adapters.
//!
//! This module exposes the command-line interface plus the runtime dispatch
//! helpers used by the legacy Tokio loop and the Wireframe transport. The
//! active runtime is selected at compile time via the `legacy-networking`
//! feature flag, allowing the bespoke frame handler to be disabled without
//! touching domain or admin flows.

pub mod admin;
pub mod cli;
#[cfg(feature = "legacy-networking")]
pub mod legacy;
pub mod wireframe;

use std::str::FromStr;

pub use admin::run_command;
use anyhow::Result;
use clap::Parser;
pub use cli::{AppConfig, Cli, Commands, CreateUserArgs};
#[cfg(feature = "legacy-networking")]
pub use legacy::run_daemon;

/// Track which networking runtime the crate is compiled to use.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkRuntime {
    Legacy,
    Wireframe,
}

/// Report the active runtime based on enabled Cargo features.
#[must_use]
pub const fn active_runtime() -> NetworkRuntime {
    if cfg!(feature = "legacy-networking") {
        NetworkRuntime::Legacy
    } else {
        NetworkRuntime::Wireframe
    }
}

impl FromStr for NetworkRuntime {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            _ if s.eq_ignore_ascii_case("legacy") => Ok(NetworkRuntime::Legacy),
            _ if s.eq_ignore_ascii_case("wireframe") => Ok(NetworkRuntime::Wireframe),
            other => Err(format!("unknown runtime '{other}'")),
        }
    }
}

/// Parse CLI arguments and execute the requested command or daemon.
///
/// # Errors
///
/// Returns any error emitted while parsing configuration or starting the
/// configured runtime.
pub async fn run() -> Result<()> {
    let cli = Cli::parse();
    run_with_cli(cli).await
}

/// Execute the server logic using an already parsed [`Cli`].
///
/// # Errors
///
/// Propagates any failure reported by the selected runtime.
#[cfg(feature = "legacy-networking")]
pub async fn run_with_cli(cli: Cli) -> Result<()> {
    match active_runtime() {
        NetworkRuntime::Legacy => legacy::dispatch(cli).await,
        NetworkRuntime::Wireframe => wireframe::run_with_cli(cli).await,
    }
}

/// Execute the server logic using an already parsed [`Cli`].
///
/// # Errors
///
/// Returns any error emitted while starting the Wireframe runtime.
#[cfg(not(feature = "legacy-networking"))]
pub async fn run_with_cli(cli: Cli) -> Result<()> { wireframe::run_with_cli(cli).await }

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[cfg(feature = "legacy-networking")]
    #[rstest]
    fn reports_legacy_runtime() {
        assert_eq!(active_runtime(), NetworkRuntime::Legacy);
    }

    #[cfg(not(feature = "legacy-networking"))]
    #[rstest]
    fn reports_wireframe_runtime() {
        assert_eq!(active_runtime(), NetworkRuntime::Wireframe);
    }
}
