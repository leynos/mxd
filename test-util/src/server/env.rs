//! Environment helpers and type-safe wrappers for server tests.

use std::{fmt, path::Path};

/// Environment variable name Cargo may set to the prebuilt server binary path.
///
/// The harness only reads it, as the fallback when a caller names no binary.
pub(super) const SERVER_BINARY_ENV: &str = "CARGO_BIN_EXE_mxd-wireframe-server";

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
