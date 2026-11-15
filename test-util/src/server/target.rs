//! What a [`TestServer`](super::TestServer) launches: a manifest, and the
//! prebuilt server binary to run when the caller names one.
//!
//! Naming the binary is an argument, not a process-wide environment variable.
//! An in-process `set_var` would force every test that starts a server to
//! queue behind one lock, and Rust 2024 makes the call `unsafe` besides. A
//! target that names no binary falls back to what the process environment
//! already holds (a read), then to `cargo run`.

use std::path::{Path, PathBuf};

use super::{binary::resolve_server_binary, env::ManifestPath};

/// The manifest to build from and, optionally, a prebuilt server binary.
#[derive(Debug, Clone)]
pub(super) struct ServerTarget {
    manifest_path: ManifestPath,
    binary: Option<PathBuf>,
}

impl ServerTarget {
    /// A target that runs `binary` when it names one, and otherwise resolves a
    /// binary from the process environment.
    pub(super) fn new(manifest_path: ManifestPath, binary: Option<&Path>) -> Self {
        Self {
            manifest_path,
            binary: binary.map(Path::to_path_buf),
        }
    }

    /// The manifest `cargo run` builds from when no binary is available.
    pub(super) const fn manifest_path(&self) -> &ManifestPath { &self.manifest_path }

    /// The prebuilt binary to run: the one named, if it is a file, else the
    /// one the process environment names.
    pub(super) fn binary(&self) -> Option<PathBuf> {
        self.binary
            .as_deref()
            .filter(|path| is_binary_file(path))
            .map(Path::to_path_buf)
            .or_else(resolve_server_binary)
    }
}

fn is_binary_file(path: &Path) -> bool {
    let is_file = path.is_file();
    if !is_file {
        tracing::warn!(binary = %path.display(), "named server binary is not a regular file");
    }
    is_file
}
