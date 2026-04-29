//! Parameter block helpers for Hotline transactions.
//!
//! The payload for most transactions is a list of parameters, each keyed by a
//! 16-bit [`FieldId`]. This module validates and serialises that parameter
//! structure.

use std::collections::{HashMap, HashSet};

use super::{FrameHeader, Transaction, errors::TransactionError, read_u16};
use crate::{field_id::FieldId, transaction_type::TransactionType};

/// Determine whether duplicate instances of the given field id are permitted.
const fn duplicate_allowed(fid: FieldId, context: DuplicateContext) -> bool {
    match fid {
        FieldId::UserNameWithInfo => context.allows_repeated_user_name_with_info,
        FieldId::NewsCategory | FieldId::NewsArticle | FieldId::FileName => true,
        _ => false,
    }
}

#[derive(Clone, Copy)]
struct DuplicateContext {
    allows_repeated_user_name_with_info: bool,
}

impl DuplicateContext {
    const DECODE_ONLY: Self = Self {
        allows_repeated_user_name_with_info: true,
    };

    const fn from_header(header: &FrameHeader) -> Self {
        Self {
            allows_repeated_user_name_with_info: header.is_reply != 0
                && header.ty == crate::transaction_type::USER_NAME_LIST_ID,
        }
    }
}

fn check_duplicate(
    fid: FieldId,
    seen: &mut HashSet<u16>,
    context: DuplicateContext,
) -> Result<(), TransactionError> {
    let raw: u16 = fid.into();
    if !duplicate_allowed(fid, context) && !seen.insert(raw) {
        return Err(TransactionError::DuplicateField(raw));
    }
    Ok(())
}

#[expect(
    clippy::indexing_slicing,
    reason = "bounds are validated before each slice"
)]
fn iter_params(
    buf: &[u8],
    duplicate_context: DuplicateContext,
) -> Result<ParamIter<'_>, TransactionError> {
    if buf.is_empty() {
        return Ok(ParamIter {
            buf,
            offset: 0,
            remaining: 0,
            seen: HashSet::new(),
            error: None,
            duplicate_context,
        });
    }
    if buf.len() < 2 {
        return Err(TransactionError::SizeMismatch);
    }
    let param_count = read_u16(&buf[0..2])? as usize;
    Ok(ParamIter {
        buf,
        offset: 2,
        remaining: param_count,
        seen: HashSet::new(),
        error: None,
        duplicate_context,
    })
}

/// Iterator over parameter entries in a buffer.
struct ParamIter<'a> {
    buf: &'a [u8],
    offset: usize,
    remaining: usize,
    seen: HashSet<u16>,
    error: Option<TransactionError>,
    duplicate_context: DuplicateContext,
}

impl Iterator for ParamIter<'_> {
    type Item = (FieldId, usize, usize);

    #[expect(
        clippy::indexing_slicing,
        reason = "bounds are validated before each slice"
    )]
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 || self.error.is_some() {
            return None;
        }
        if self.offset + 4 > self.buf.len() {
            self.error = Some(TransactionError::SizeMismatch);
            return None;
        }
        let field_id = match read_u16(&self.buf[self.offset..self.offset + 2]) {
            Ok(id) => id,
            Err(e) => {
                self.error = Some(e);
                return None;
            }
        };
        let field_len = match read_u16(&self.buf[self.offset + 2..self.offset + 4]) {
            Ok(len) => len as usize,
            Err(e) => {
                self.error = Some(e);
                return None;
            }
        };
        self.offset += 4;
        let start = self.offset;
        if start + field_len > self.buf.len() {
            self.error = Some(TransactionError::SizeMismatch);
            return None;
        }
        let fid = FieldId::from(field_id);
        if let Err(e) = check_duplicate(fid, &mut self.seen, self.duplicate_context) {
            self.error = Some(e);
            return None;
        }
        self.offset += field_len;
        self.remaining -= 1;
        Some((fid, start, field_len))
    }
}

impl ParamIter<'_> {
    /// Check if iteration completed successfully and the buffer was fully consumed.
    fn finish(self, buf_len: usize) -> Result<(), TransactionError> {
        if let Some(e) = self.error {
            return Err(e);
        }
        if self.offset != buf_len {
            return Err(TransactionError::SizeMismatch);
        }
        Ok(())
    }
}

/// Validate the assembled transaction payload for duplicate fields and length
/// correctness according to the protocol specification.
///
/// # Errors
///
/// Returns an error if the payload structure is invalid.
pub fn validate_payload(tx: &Transaction) -> Result<(), TransactionError> {
    validate_payload_parts(&tx.header, &tx.payload)
}

/// Validate a transaction payload slice against its header.
///
/// This helper lets callers validate parameter blocks without constructing a
/// full [`Transaction`] value.
///
/// # Errors
///
/// Returns an error if the payload structure is invalid.
pub fn validate_payload_parts(
    header: &FrameHeader,
    payload: &[u8],
) -> Result<(), TransactionError> {
    if header.total_size as usize != payload.len() {
        return Err(TransactionError::SizeMismatch);
    }
    if payload.is_empty() {
        return Ok(());
    }
    if header.is_reply == 0 && TransactionType::from(header.ty).bypass_payload_decode() {
        return Ok(());
    }
    let mut iter = iter_params(payload, DuplicateContext::from_header(header))?;
    // Consume the iterator to validate all parameters
    for _ in &mut iter {}
    iter.finish(payload.len())
}

/// Decode the parameter block into a vector of field id/value pairs.
///
/// # Errors
/// Returns an error if the buffer cannot be parsed.
#[must_use = "handle the result"]
#[expect(
    clippy::indexing_slicing,
    reason = "bounds are validated by iter_params"
)]
pub fn decode_params(buf: &[u8]) -> Result<Vec<(FieldId, Vec<u8>)>, TransactionError> {
    if buf.is_empty() {
        return Ok(Vec::new());
    }
    let mut iter = iter_params(buf, DuplicateContext::DECODE_ONLY)?;
    let mut params = Vec::new();
    for (fid, start, len) in &mut iter {
        params.push((fid, buf[start..start + len].to_vec()));
    }
    iter.finish(buf.len())?;
    Ok(params)
}

/// Decode the parameter block into a map keyed by `FieldId`.
///
/// # Errors
/// Returns an error if the buffer cannot be parsed.
#[must_use = "handle the result"]
pub fn decode_params_map(buf: &[u8]) -> Result<HashMap<FieldId, Vec<Vec<u8>>>, TransactionError> {
    let params = decode_params(buf)?;
    let mut map: HashMap<FieldId, Vec<Vec<u8>>> = HashMap::new();
    for (fid, value) in params {
        map.entry(fid).or_default().push(value);
    }
    Ok(map)
}

/// Build a parameter block from field id/data pairs.
///
/// Accepts any slice of pairs where the second element can be borrowed as `&[u8]`,
/// allowing both `&[(FieldId, &[u8])]` and `&[(FieldId, Vec<u8>)]`.
///
/// # Errors
/// Returns [`TransactionError::PayloadTooLarge`] if the number of parameters
/// or any data length exceeds `u16::MAX`.
#[must_use = "use the encoded bytes"]
#[expect(clippy::big_endian_bytes, reason = "network protocol uses big-endian")]
pub fn encode_params<T: AsRef<[u8]>>(params: &[(FieldId, T)]) -> Result<Vec<u8>, TransactionError> {
    let mut buf = Vec::new();
    buf.extend_from_slice(
        &u16::try_from(params.len())
            .map_err(|_| TransactionError::PayloadTooLarge)?
            .to_be_bytes(),
    );
    for (id, data) in params {
        let raw: u16 = (*id).into();
        let data_bytes = data.as_ref();
        buf.extend_from_slice(&raw.to_be_bytes());
        buf.extend_from_slice(
            &u16::try_from(data_bytes.len())
                .map_err(|_| TransactionError::PayloadTooLarge)?
                .to_be_bytes(),
        );
        buf.extend_from_slice(data_bytes);
    }
    Ok(buf)
}

/// Retrieve the first value for `field` from a parameter map.
///
/// Returns `None` if the field is absent.
fn first_value<S: std::hash::BuildHasher>(
    map: &HashMap<FieldId, Vec<Vec<u8>>, S>,
    field: FieldId,
) -> Option<&[u8]> {
    map.get(&field).and_then(|v| v.first()).map(Vec::as_slice)
}

/// Return the first value for `field` in a parameter map as a `String`.
///
/// Returns `Ok(None)` if the field is absent and an error if the bytes are not
/// valid UTF-8.
///
/// # Errors
/// Returns [`TransactionError::InvalidParamValue`] if the parameter value is
/// not valid UTF-8.
#[must_use = "handle the result"]
pub fn first_param_string<S: std::hash::BuildHasher>(
    map: &HashMap<FieldId, Vec<Vec<u8>>, S>,
    field: FieldId,
) -> Result<Option<String>, TransactionError> {
    match first_value(map, field) {
        Some(bytes) => Ok(Some(
            std::str::from_utf8(bytes)
                .map_err(|_| TransactionError::InvalidParamValue(field))?
                .to_owned(),
        )),
        None => Ok(None),
    }
}

/// Return the first value for `field` as a `String` or an error if missing.
///
/// # Errors
/// Returns [`TransactionError::MissingField`] if the field is absent, or
/// [`TransactionError::InvalidParamValue`] if the value is not valid UTF-8.
#[must_use = "handle the result"]
pub fn required_param_string<S: std::hash::BuildHasher>(
    map: &HashMap<FieldId, Vec<Vec<u8>>, S>,
    field: FieldId,
) -> Result<String, TransactionError> {
    first_param_string(map, field)?.ok_or(TransactionError::MissingField(field))
}

/// Decode the first value for `field` as a big-endian `i32`.
///
/// # Errors
/// Returns [`TransactionError::MissingField`] if the field is absent, or
/// [`TransactionError::InvalidParamValue`] if the value cannot be parsed as `i32`.
#[must_use = "handle the result"]
#[expect(clippy::big_endian_bytes, reason = "network protocol uses big-endian")]
pub fn required_param_i32<S: std::hash::BuildHasher>(
    map: &HashMap<FieldId, Vec<Vec<u8>>, S>,
    field: FieldId,
) -> Result<i32, TransactionError> {
    let bytes = first_value(map, field).ok_or(TransactionError::MissingField(field))?;
    let arr: [u8; 4] = bytes
        .try_into()
        .map_err(|_| TransactionError::InvalidParamValue(field))?;
    Ok(i32::from_be_bytes(arr))
}

/// Decode the first value for `field` as an `i32` if present.
///
/// Returns `Ok(None)` if the parameter is absent and an error if it is present
/// but does not decode as a big-endian `i32`.
///
/// # Errors
/// Returns [`TransactionError::InvalidParamValue`] if the value cannot be
/// parsed as `i32`.
#[must_use = "handle the result"]
#[expect(clippy::big_endian_bytes, reason = "network protocol uses big-endian")]
pub fn first_param_i32<S: std::hash::BuildHasher>(
    map: &HashMap<FieldId, Vec<Vec<u8>>, S>,
    field: FieldId,
) -> Result<Option<i32>, TransactionError> {
    match first_value(map, field) {
        Some(bytes) => {
            let arr: [u8; 4] = bytes
                .try_into()
                .map_err(|_| TransactionError::InvalidParamValue(field))?;
            Ok(Some(i32::from_be_bytes(arr)))
        }
        None => Ok(None),
    }
}

/// Decode the first value for `field` as a big-endian `u32`, accepting either
/// 16-bit or 32-bit protocol encodings.
///
/// # Errors
/// Returns [`TransactionError::MissingField`] if the field is absent, or
/// [`TransactionError::InvalidParamValue`] if the value cannot be parsed as a
/// 16-bit or 32-bit big-endian unsigned integer.
#[must_use = "handle the result"]
pub fn required_param_u32<S: std::hash::BuildHasher>(
    map: &HashMap<FieldId, Vec<Vec<u8>>, S>,
    field: FieldId,
) -> Result<u32, TransactionError> {
    first_param_u32(map, field)?.ok_or(TransactionError::MissingField(field))
}

/// Decode the first value for `field` as a `u32` if present, accepting either
/// 16-bit or 32-bit protocol encodings.
///
/// # Errors
/// Returns [`TransactionError::InvalidParamValue`] if the value length is not
/// two or four bytes.
#[must_use = "handle the result"]
pub fn first_param_u32<S: std::hash::BuildHasher>(
    map: &HashMap<FieldId, Vec<Vec<u8>>, S>,
    field: FieldId,
) -> Result<Option<u32>, TransactionError> {
    match first_value(map, field) {
        Some(bytes) => Ok(Some(parse_protocol_u32(bytes, field)?)),
        None => Ok(None),
    }
}

fn parse_protocol_u32(bytes: &[u8], field: FieldId) -> Result<u32, TransactionError> {
    match bytes.len() {
        2 => parse_protocol_u16(bytes, field).map(u32::from),
        4 => parse_protocol_u32_exact(bytes, field),
        _ => Err(TransactionError::InvalidParamValue(field)),
    }
}

#[expect(clippy::big_endian_bytes, reason = "network protocol uses big-endian")]
fn parse_protocol_u16(bytes: &[u8], field: FieldId) -> Result<u16, TransactionError> {
    let arr: [u8; 2] = bytes
        .try_into()
        .map_err(|_| TransactionError::InvalidParamValue(field))?;
    Ok(u16::from_be_bytes(arr))
}

#[expect(clippy::big_endian_bytes, reason = "network protocol uses big-endian")]
fn parse_protocol_u32_exact(bytes: &[u8], field: FieldId) -> Result<u32, TransactionError> {
    let arr: [u8; 4] = bytes
        .try_into()
        .map_err(|_| TransactionError::InvalidParamValue(field))?;
    Ok(u32::from_be_bytes(arr))
}
