//! Parameter block helpers for Hotline transactions.
//!
//! The payload for most transactions is a list of parameters, each keyed by a
//! 16-bit [`FieldId`]. This module validates and serialises that parameter
//! structure.

#![expect(clippy::big_endian_bytes, reason = "network protocol uses big-endian")]
#![expect(
    clippy::indexing_slicing,
    reason = "array bounds are validated earlier in parsing"
)]
use std::collections::HashSet;

use super::{Transaction, errors::TransactionError, read_u16};
use crate::field_id::FieldId;

/// Determine whether duplicate instances of the given field id are permitted.
const fn duplicate_allowed(fid: FieldId) -> bool {
    matches!(
        fid,
        FieldId::NewsCategory | FieldId::NewsArticle | FieldId::FileName
    )
}

fn check_duplicate(fid: FieldId, seen: &mut HashSet<u16>) -> Result<(), TransactionError> {
    let raw: u16 = fid.into();
    if !duplicate_allowed(fid) && !seen.insert(raw) {
        return Err(TransactionError::DuplicateField(raw));
    }
    Ok(())
}

/// Validate the assembled transaction payload for duplicate fields and length
/// correctness according to the protocol specification.
///
/// # Errors
///
/// Returns an error if the payload structure is invalid.
pub fn validate_payload(tx: &Transaction) -> Result<(), TransactionError> {
    if tx.header.total_size as usize != tx.payload.len() {
        return Err(TransactionError::SizeMismatch);
    }
    if tx.payload.is_empty() {
        return Ok(());
    }
    if tx.payload.len() < 2 {
        return Err(TransactionError::SizeMismatch);
    }
    let param_count = read_u16(&tx.payload[0..2])? as usize;
    let mut offset = 2;
    let mut seen = HashSet::new();
    for _ in 0..param_count {
        if offset + 4 > tx.payload.len() {
            return Err(TransactionError::SizeMismatch);
        }
        let field_id = read_u16(&tx.payload[offset..offset + 2])?;
        let field_size = read_u16(&tx.payload[offset + 2..offset + 4])? as usize;
        offset += 4;
        if offset + field_size > tx.payload.len() {
            return Err(TransactionError::SizeMismatch);
        }
        let fid = FieldId::from(field_id);
        check_duplicate(fid, &mut seen)?;
        offset += field_size;
    }
    if offset != tx.payload.len() {
        return Err(TransactionError::SizeMismatch);
    }
    Ok(())
}

/// Decode the parameter block into a vector of field id/value pairs.
///
/// # Errors
/// Returns an error if the buffer cannot be parsed.
#[must_use = "handle the result"]
pub fn decode_params(buf: &[u8]) -> Result<Vec<(FieldId, Vec<u8>)>, TransactionError> {
    if buf.is_empty() {
        return Ok(Vec::new());
    }
    if buf.len() < 2 {
        return Err(TransactionError::SizeMismatch);
    }
    let param_count = read_u16(&buf[0..2])? as usize;
    let mut offset = 2;
    let mut params = Vec::with_capacity(param_count);
    let mut seen = HashSet::new();
    for _ in 0..param_count {
        if offset + 4 > buf.len() {
            return Err(TransactionError::SizeMismatch);
        }
        let field_id = read_u16(&buf[offset..offset + 2])?;
        let field_len = read_u16(&buf[offset + 2..offset + 4])? as usize;
        offset += 4;
        if offset + field_len > buf.len() {
            return Err(TransactionError::SizeMismatch);
        }
        let fid = FieldId::from(field_id);
        check_duplicate(fid, &mut seen)?;
        params.push((fid, buf[offset..offset + field_len].to_vec()));
        offset += field_len;
    }
    if offset != buf.len() {
        return Err(TransactionError::SizeMismatch);
    }
    Ok(params)
}

/// Decode the parameter block into a map keyed by `FieldId`.
///
/// # Errors
/// Returns an error if the buffer cannot be parsed.
#[must_use = "handle the result"]
pub fn decode_params_map(
    buf: &[u8],
) -> Result<std::collections::HashMap<FieldId, Vec<Vec<u8>>>, TransactionError> {
    let params = decode_params(buf)?;
    let mut map: std::collections::HashMap<FieldId, Vec<Vec<u8>>> =
        std::collections::HashMap::new();
    for (fid, value) in params {
        map.entry(fid).or_default().push(value);
    }
    Ok(map)
}

/// Build a parameter block from field id/data pairs.
///
/// # Errors
/// Returns [`TransactionError::PayloadTooLarge`] if the number of parameters
/// or any data length exceeds `u16::MAX`.
#[must_use = "use the encoded bytes"]
pub fn encode_params(params: &[(FieldId, &[u8])]) -> Result<Vec<u8>, TransactionError> {
    let mut buf = Vec::new();
    buf.extend_from_slice(
        &u16::try_from(params.len())
            .map_err(|_| TransactionError::PayloadTooLarge)?
            .to_be_bytes(),
    );
    for (id, data) in params {
        let raw: u16 = (*id).into();
        buf.extend_from_slice(&raw.to_be_bytes());
        buf.extend_from_slice(
            &u16::try_from(data.len())
                .map_err(|_| TransactionError::PayloadTooLarge)?
                .to_be_bytes(),
        );
        buf.extend_from_slice(data);
    }
    Ok(buf)
}

/// Convenience for encoding a vector of owned parameter values.
///
/// This converts a `&[(FieldId, Vec<u8>)]` slice into the borrowed
/// form expected by [`encode_params`]. It avoids repeating the
/// conversion logic at call sites.
///
/// # Errors
/// Returns [`TransactionError`] if the inner call to [`encode_params`]
/// fails, for example when the payload is too large.
#[must_use = "use the encoded bytes"]
pub fn encode_vec_params(params: &[(FieldId, Vec<u8>)]) -> Result<Vec<u8>, TransactionError> {
    let borrowed: Vec<(FieldId, &[u8])> = params
        .iter()
        .map(|(id, bytes)| (*id, bytes.as_slice()))
        .collect();
    encode_params(&borrowed)
}

/// Return the first value for `field` in a parameter map as a `String`.
///
/// Returns `Ok(None)` if the field is absent and an error if the bytes are not
/// valid UTF-8.
///
/// # Errors
/// Returns an error if the parameter value is not valid UTF-8.
#[must_use = "handle the result"]
pub fn first_param_string<S: std::hash::BuildHasher>(
    map: &std::collections::HashMap<FieldId, Vec<Vec<u8>>, S>,
    field: FieldId,
) -> Result<Option<String>, &'static str> {
    match map.get(&field).and_then(|v| v.first()) {
        Some(bytes) => Ok(Some(String::from_utf8(bytes.clone()).map_err(|_| "utf8")?)),
        None => Ok(None),
    }
}

/// Return the first value for `field` as a `String` or an error if missing.
///
/// # Errors
/// Returns an error if the field is missing or not valid UTF-8.
#[must_use = "handle the result"]
pub fn required_param_string<S: std::hash::BuildHasher>(
    map: &std::collections::HashMap<FieldId, Vec<Vec<u8>>, S>,
    field: FieldId,
    missing_err: &'static str,
) -> Result<String, &'static str> {
    first_param_string(map, field)?.ok_or(missing_err)
}

/// Decode the first value for `field` as a big-endian `i32`.
///
/// # Errors
/// Returns an error if the field is missing or cannot be parsed as `i32`.
#[must_use = "handle the result"]
pub fn required_param_i32<S: std::hash::BuildHasher>(
    map: &std::collections::HashMap<FieldId, Vec<Vec<u8>>, S>,
    field: FieldId,
    missing_err: &'static str,
    parse_err: &'static str,
) -> Result<i32, &'static str> {
    let bytes = map.get(&field).and_then(|v| v.first()).ok_or(missing_err)?;
    let arr: [u8; 4] = bytes.as_slice().try_into().map_err(|_| parse_err)?;
    Ok(i32::from_be_bytes(arr))
}

/// Decode the first value for `field` as an `i32` if present.
///
/// Returns `Ok(None)` if the parameter is absent and an error if it is present
/// but does not decode as a big-endian `i32`.
///
/// # Errors
/// Returns an error if the value cannot be parsed as `i32`.
#[must_use = "handle the result"]
pub fn first_param_i32<S: std::hash::BuildHasher>(
    map: &std::collections::HashMap<FieldId, Vec<Vec<u8>>, S>,
    field: FieldId,
    parse_err: &'static str,
) -> Result<Option<i32>, &'static str> {
    match map.get(&field).and_then(|v| v.first()) {
        Some(bytes) => {
            let arr: [u8; 4] = bytes.as_slice().try_into().map_err(|_| parse_err)?;
            Ok(Some(i32::from_be_bytes(arr)))
        }
        None => Ok(None),
    }
}
