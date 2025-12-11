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
    /// Active when the `legacy-networking` feature is enabled (uses the legacy
    /// networking implementation).
    Legacy,
    /// Active when the `legacy-networking` feature is not enabled (uses the
    /// wireframe networking implementation).
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
        let lower = s.to_ascii_lowercase();
        match lower.as_str() {
            "legacy" => Ok(Self::Legacy),
            "wireframe" => Ok(Self::Wireframe),
            _ => Err(format!("unknown runtime '{s}'")),
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
pub async fn run_with_cli(cli: Cli) -> Result<()> { legacy::dispatch(cli).await }

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
    #[test]
    fn reports_legacy_runtime() {
        assert_eq!(active_runtime(), NetworkRuntime::Legacy);
    }

    #[cfg(not(feature = "legacy-networking"))]
    #[test]
    fn reports_wireframe_runtime() {
        assert_eq!(active_runtime(), NetworkRuntime::Wireframe);
    }

    #[rstest]
    #[case("LeGaCy", NetworkRuntime::Legacy)]
    #[case("wireFRAME", NetworkRuntime::Wireframe)]
    fn parses_known_runtimes(#[case] input: &str, #[case] expected: NetworkRuntime) {
        let parsed = input
            .parse::<NetworkRuntime>()
            .expect("runtime string should parse");
        assert_eq!(parsed, expected);
    }

    #[test]
    fn rejects_unknown_runtime() {
        assert!("unknown".parse::<NetworkRuntime>().is_err());
    }
}
