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
    ///
    /// The named binary is returned as an absolute path, because
    /// `Command::new` searches `PATH` for a bare name such as `server-test`
    /// even when a file of that name sits in the working directory.
    pub(super) fn binary(&self) -> Option<PathBuf> {
        self.binary
            .as_deref()
            .filter(|path| is_binary_file(path))
            .map(launchable_path)
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

/// The path to hand to `Command::new` so the OS runs this file and does not
/// search `PATH` for its name.
fn launchable_path(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    //! The named binary is the one launched; a path that is not a file is not.

    use std::{
        net::{Ipv4Addr, SocketAddr},
        path::Path,
    };

    use rstest::rstest;
    use tempfile::TempDir;

    use super::{ServerTarget, launchable_path};
    use crate::server::{DbUrl, ManifestPath, build_server_command};

    fn target(binary: Option<&Path>) -> ServerTarget {
        ServerTarget::new(ManifestPath::from("./Cargo.toml"), binary)
    }

    fn launch_command(target: &ServerTarget) -> std::process::Command {
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 4321));
        build_server_command(target, addr, &DbUrl::from("test.db"))
    }

    #[rstest]
    fn a_named_file_is_the_program_that_launches() {
        let dir = TempDir::new().expect("temporary directory");
        let binary = dir.path().join("distinguishable-server");
        std::fs::write(&binary, b"").expect("write the stand-in binary");

        let command = launch_command(&target(Some(&binary)));

        assert_eq!(command.get_program(), binary.as_os_str());
        let arguments: Vec<_> = command.get_args().collect();
        assert_eq!(
            arguments,
            ["--bind", "127.0.0.1:4321", "--database", "test.db"]
        );
    }

    #[rstest]
    fn a_named_path_that_is_not_a_file_falls_back() {
        let dir = TempDir::new().expect("temporary directory");
        let missing = dir.path().join("no-such-server");

        let program_is_missing_path =
            launch_command(&target(Some(&missing))).get_program() == missing.as_os_str();

        assert!(
            !program_is_missing_path,
            "a missing binary must not be launched"
        );
        assert_ne!(target(Some(&missing)).binary(), Some(missing));
    }

    #[rstest]
    #[case::bare_name("server-test")]
    #[case::dot_slash("./server-test")]
    fn a_relative_name_is_launched_by_absolute_path(#[case] name: &str) {
        let launchable = launchable_path(Path::new(name));

        assert!(
            launchable.is_absolute(),
            "{launchable:?} must not be searched on PATH"
        );
        assert!(launchable.ends_with("server-test"));
    }

    #[rstest]
    fn a_named_relative_file_is_returned_by_absolute_path() {
        let dir = TempDir::new_in(".").expect("temporary directory in the working directory");
        let absolute = dir.path().join("relative-server");
        std::fs::write(&absolute, b"").expect("write the stand-in binary");
        let working_directory = std::env::current_dir().expect("working directory");
        let binary = absolute
            .strip_prefix(working_directory)
            .expect("the temporary directory is inside the working directory");
        assert!(
            binary.is_relative(),
            "{binary:?} should be relative to the working directory"
        );

        let found = target(Some(binary)).binary();

        assert!(
            found
                .as_deref()
                .is_some_and(|path| path.is_absolute() && path.ends_with("relative-server")),
            "{found:?} should be the file, by absolute path"
        );
    }
}
