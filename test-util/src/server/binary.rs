//! Server binary resolution helpers for the integration harness.

use std::path::{Path, PathBuf};

use tracing::debug;

use super::env::SERVER_BINARY_ENV;

pub(super) fn resolve_server_binary() -> Option<PathBuf> {
    let resolution =
        std::env::var_os(SERVER_BINARY_ENV).map_or(ServerBinaryResolution::EnvMissing, |bin| {
            let path = PathBuf::from(bin);
            if path.is_file() {
                ServerBinaryResolution::Found(path)
            } else {
                ServerBinaryResolution::Missing(path)
            }
        });
    resolution.log();
    resolution.into_option()
}

enum ServerBinaryResolution {
    EnvMissing,
    Found(PathBuf),
    Missing(PathBuf),
}

impl ServerBinaryResolution {
    fn log(&self) {
        let (message, binary) = match self {
            Self::EnvMissing => ("env var not set", None),
            Self::Found(path) => ("using prebuilt binary", Some(path.as_path())),
            Self::Missing(path) => (
                "binary from env var does not exist or is not a regular file",
                Some(path.as_path()),
            ),
        };
        log_server_binary_resolution(message, binary);
    }

    fn into_option(self) -> Option<PathBuf> {
        match self {
            Self::Found(path) => Some(path),
            Self::EnvMissing | Self::Missing(_) => None,
        }
    }
}

fn log_server_binary_resolution(message: &'static str, binary: Option<&Path>) {
    let binary_display = binary.map(|path| path.display().to_string());
    debug!(
        env_var = SERVER_BINARY_ENV,
        binary = ?binary_display,
        "{message}"
    );
}
