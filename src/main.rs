//! Binary entry point for the legacy TCP server.
//!
//! All runtime logic lives in `mxd::server`, allowing future binaries to re-use
//! the same domain modules and configuration plumbing.

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> { mxd::server::run().await }
