//! Binary entry point for the legacy TCP server.
//!
//! All runtime logic lives in `mxd::server`, allowing future binaries to re-use
//! the same domain modules and configuration plumbing.

use anyhow::{Context, Result};
use tokio::runtime::Builder;

fn main() -> Result<()> {
    let runtime = Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("failed to build Tokio runtime")?;
    runtime.block_on(async { mxd::server::run().await })
}
