//! Shared helpers for resolving bundle/category paths.

use thiserror::Error;

use crate::news_path::prepare_path;

/// Errors that can occur when resolving news paths.
#[derive(Debug, Error)]
pub enum PathLookupError {
    /// The provided news path is invalid or malformed.
    #[error("invalid news path")]
    InvalidPath,
    /// A database query error occurred.
    #[error(transparent)]
    Diesel(#[from] diesel::result::Error),
    /// A JSON serialisation error occurred.
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}

/// Parse a news path into JSON segments, enforcing empty-path rules.
pub fn parse_path_segments(
    path: &str,
    allow_empty: bool,
) -> Result<Option<(String, usize)>, PathLookupError> {
    let Some((json, len)) = prepare_path(path)? else {
        return if allow_empty {
            Ok(None)
        } else {
            Err(PathLookupError::InvalidPath)
        };
    };
    Ok(Some((json, len)))
}

/// Validate the lookup result, returning an error when a match is required.
pub const fn normalize_lookup_result(
    id: Option<i32>,
    require_match: bool,
) -> Result<Option<i32>, PathLookupError> {
    if require_match && id.is_none() {
        Err(PathLookupError::InvalidPath)
    } else {
        Ok(id)
    }
}
