//! Binary entry point for the Wireframe-based server.
//!
//! The runtime logic lives in `mxd::server::wireframe`, so this binary only
//! delegates to the shared library code.

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> { mxd::server::wireframe::run().await }
