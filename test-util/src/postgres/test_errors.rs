//! Deterministic nested bootstrap errors for retry regression tests.

use std::{error::Error, fmt, io};

/// An outer error that deliberately hides its cause from its display text.
#[derive(Debug)]
pub(super) struct NestedError(io::Error);

impl NestedError {
    /// Construct an outer bootstrap error with the requested underlying cause.
    pub(super) fn new(cause: &str) -> Self { Self(io::Error::other(cause.to_owned())) }
}

impl fmt::Display for NestedError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("bootstrap failed")
    }
}

impl Error for NestedError {
    fn source(&self) -> Option<&(dyn Error + 'static)> { Some(&self.0) }
}
