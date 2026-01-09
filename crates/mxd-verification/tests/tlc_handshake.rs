//! Integration test validating TLC runs the handshake spec without violations.
//!
//! This test invokes TLC via the Docker wrapper script and verifies that the
//! handshake specification passes all invariant checks. It is marked `#[ignore]`
//! by default because it requires Docker and takes several seconds to run.
//!
//! Run with: `cargo test -p mxd-verification -- --ignored`
//! Or via Make: `make tlc-handshake`

use std::process::Command;

/// Run TLC on the handshake spec and verify no invariant violations.
///
/// This test requires Docker to be available. It runs the TLC model checker
/// via `scripts/run-tlc.sh` and asserts that:
/// - TLC exits with code 0 (success)
/// - Output contains "Model checking completed" (TLC ran to completion)
/// - Output does not contain "Error:" or "Invariant .* is violated"
///
/// # Panics
///
/// Panics if TLC reports invariant violations or fails to run.
#[test]
#[ignore = "requires Docker and takes several seconds to run"]
fn tlc_handshake_no_violations() {
    // Check Docker is available
    let docker_check = Command::new("docker")
        .arg("--version")
        .output()
        .expect("Docker must be installed to run TLC tests");

    assert!(
        docker_check.status.success(),
        "Docker is not available: {}",
        String::from_utf8_lossy(&docker_check.stderr)
    );

    // Run TLC via the Docker wrapper
    //
    // Resolve the workspace root relative to this crate's manifest directory:
    //   <workspace_root>/crates/mxd-verification -> CARGO_MANIFEST_DIR
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("Failed to determine workspace root from CARGO_MANIFEST_DIR");

    // Build the path to the TLC wrapper script at <workspace_root>/scripts/run-tlc.sh
    let script_path = workspace_root.join("scripts").join("run-tlc.sh");

    let output = Command::new(&script_path)
        .arg("crates/mxd-verification/tla/MxdHandshake.tla")
        .current_dir(workspace_root)
        .output()
        .expect("Failed to execute run-tlc.sh");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined_output = format!("{stdout}\n{stderr}");

    // TLC should complete without errors
    assert!(
        !combined_output.contains("Error:"),
        "TLC reported an error:\n{combined_output}"
    );

    assert!(
        !combined_output.contains("is violated"),
        "TLC found an invariant violation:\n{combined_output}"
    );

    // TLC should exit successfully
    assert!(
        output.status.success(),
        "TLC exited with non-zero status: {}\n\nOutput:\n{combined_output}",
        output.status
    );

    // TLC should report completion
    assert!(
        combined_output.contains("Model checking completed")
            || combined_output.contains("Finished in"),
        "TLC did not complete model checking:\n{combined_output}"
    );
}
