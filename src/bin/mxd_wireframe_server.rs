//! Binary entry point for the Wireframe-based server.
//!
//! The runtime logic lives in `mxd::server::wireframe`, so this binary only
//! delegates to the shared library code.

use std::process::ExitCode;

use tokio::runtime::Builder;

#[expect(
    clippy::print_stderr,
    reason = "error output is appropriate for main binary"
)]
fn main() -> ExitCode {
    let runtime = match Builder::new_multi_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(err) => {
            eprintln!("mxd-wireframe-server failed to build runtime: {err:#}");
            return ExitCode::FAILURE;
        }
    };

    runtime.block_on(async {
        match mxd::server::wireframe::run().await {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("mxd-wireframe-server failed: {err:#}");
                ExitCode::FAILURE
            }
        }
    })
}
