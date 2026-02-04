//! Environment helpers and type-safe wrappers for server tests.

use std::{ffi::OsString, fmt, io, path::Path, sync::Mutex};

use crate::AnyError;

/// Environment variable name for the prebuilt server binary path.
pub(super) const SERVER_BINARY_ENV: &str = "CARGO_BIN_EXE_mxd-wireframe-server";

static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Newtype wrapping the path to a Cargo manifest, providing type-safe handling
/// and ergonomic conversions.
#[derive(Debug, Clone)]
pub struct ManifestPath(String);

impl ManifestPath {
    /// Constructs a new manifest path from any string-like type.
    pub fn new(path: impl Into<String>) -> Self { Self(path.into()) }
    /// Returns the path as a string slice.
    pub fn as_str(&self) -> &str { &self.0 }
}

impl From<&str> for ManifestPath {
    fn from(value: &str) -> Self { Self(value.to_owned()) }
}

impl From<String> for ManifestPath {
    fn from(value: String) -> Self { Self(value) }
}

impl AsRef<str> for ManifestPath {
    fn as_ref(&self) -> &str { &self.0 }
}

impl AsRef<Path> for ManifestPath {
    fn as_ref(&self) -> &Path { Path::new(&self.0) }
}

impl fmt::Display for ManifestPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}", self.0) }
}

/// Newtype wrapping a database connection URL that provides ergonomic
/// conversions for type-safe handling.
#[derive(Debug, Clone)]
pub struct DbUrl(String);

impl DbUrl {
    /// Constructs a new database URL from any string-like type.
    pub fn new(url: impl Into<String>) -> Self { Self(url.into()) }
    /// Returns the URL as a string slice.
    pub fn as_str(&self) -> &str { &self.0 }
}

impl From<&str> for DbUrl {
    fn from(value: &str) -> Self { Self(value.to_owned()) }
}

impl From<String> for DbUrl {
    fn from(value: String) -> Self { Self(value) }
}

impl AsRef<str> for DbUrl {
    fn as_ref(&self) -> &str { &self.0 }
}

impl fmt::Display for DbUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}", self.0) }
}

/// Ensure the server binary environment variable is populated from the provided
/// compile-time path.
///
/// The mutation is guarded by a global mutex and the result is propagated so
/// callers can handle synchronization failures instead of panicking.
///
/// # Errors
///
/// Returns an error if the environment mutex is poisoned.
pub fn ensure_server_binary_env(bin_path: &str) -> Result<(), AnyError> {
    let _guard = ENV_LOCK
        .lock()
        .map_err(|_| io::Error::other("environment mutex poisoned"))?;
    if std::env::var_os(SERVER_BINARY_ENV).is_none() {
        // SAFETY: Environment mutation is serialized by `ENV_LOCK`, ensuring no
        // concurrent readers/writers observe a partially updated state.
        unsafe { std::env::set_var(SERVER_BINARY_ENV, bin_path) };
    }
    Ok(())
}

struct EnvVarGuard {
    key: String,
    previous: Option<OsString>,
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => {
                // SAFETY: Environment mutation is serialized by `ENV_LOCK`.
                unsafe { std::env::set_var(&self.key, value) };
            }
            None => {
                // SAFETY: Environment mutation is serialized by `ENV_LOCK`.
                unsafe { std::env::remove_var(&self.key) };
            }
        }
    }
}

/// Temporarily sets an environment variable for the duration of a closure.
///
/// The mutation is guarded by a global mutex and is restored after the closure
/// returns, even when it returns an error.
///
/// # Errors
///
/// Returns an error if the environment mutex is poisoned or the closure fails.
pub fn with_env_var<T>(
    key: &str,
    value: Option<&str>,
    f: impl FnOnce() -> Result<T, AnyError>,
) -> Result<T, AnyError> {
    let _guard = ENV_LOCK
        .lock()
        .map_err(|_| io::Error::other("environment mutex poisoned"))?;
    let previous = std::env::var_os(key);
    match value {
        Some(env_value) => {
            // SAFETY: Environment mutation is serialized by `ENV_LOCK`.
            unsafe { std::env::set_var(key, env_value) };
        }
        None => {
            // SAFETY: Environment mutation is serialized by `ENV_LOCK`.
            unsafe { std::env::remove_var(key) };
        }
    }
    let restore = EnvVarGuard {
        key: key.to_owned(),
        previous,
    };
    let result = f();
    drop(restore);
    result
}
