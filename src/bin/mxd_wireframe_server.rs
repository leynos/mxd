//! Binary entry point for the Wireframe-based server.
//!
//! The runtime logic lives in `mxd::server::wireframe`, so this binary only
//! delegates to the shared library code.

use std::process::ExitCode;

#[expect(
    clippy::print_stderr,
    reason = "error output is appropriate for main binary"
)]
#[tokio::main]
async fn main() -> ExitCode {
    match mxd::server::wireframe::run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("mxd-wireframe-server failed: {err:#}");
            ExitCode::FAILURE
        }
    }
}
