//! Compatibility shims for legacy Hotline clients.
//!
//! This module hosts the XOR text-field compatibility layer required by
//! clients that obfuscate text parameters by XOR-ing each byte with `0xFF`.
//! The shim detects XOR-encoded inputs, transparently decodes inbound payloads
//! and encodes outbound payloads when required, while keeping the domain layer
//! unaware of the client-specific behaviour.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::{
    field_id::FieldId,
    transaction::{TransactionError, decode_params, encode_params},
    wireframe::connection::HandshakeMetadata,
};

/// Per-connection XOR compatibility state.
#[derive(Debug)]
pub struct XorCompatibility {
    enabled: AtomicBool,
}

impl XorCompatibility {
    /// Construct a compatibility state seeded from handshake metadata.
    ///
    /// This currently ignores the provided metadata and defaults to XOR
    /// disabled. It exists as a placeholder until a reliable handshake-based
    /// XOR detection rule is available.
    #[must_use]
    pub const fn from_handshake(_handshake: &HandshakeMetadata) -> Self {
        Self {
            enabled: AtomicBool::new(false),
        }
    }

    /// Construct a compatibility state with XOR disabled.
    #[must_use]
    pub const fn disabled() -> Self {
        Self {
            enabled: AtomicBool::new(false),
        }
    }

    /// Construct a compatibility state with XOR enabled.
    #[must_use]
    pub const fn enabled() -> Self {
        Self {
            enabled: AtomicBool::new(true),
        }
    }

    /// Returns `true` when XOR encoding is enabled for this connection.
    #[must_use]
    pub fn is_enabled(&self) -> bool { self.enabled.load(Ordering::Relaxed) }

    fn enable(&self) { self.enabled.store(true, Ordering::Relaxed); }

    /// Decode a parameter payload, transparently XOR-decoding text fields.
    ///
    /// When XOR is already enabled, all text fields are decoded. Otherwise, the
    /// decoder checks whether XOR-ing the text fields yields valid UTF-8 and
    /// enables the compatibility mode on success.
    ///
    /// # Errors
    ///
    /// Returns an error if the parameter payload cannot be decoded or
    /// re-encoded.
    pub fn decode_payload(&self, payload: &[u8]) -> Result<Vec<u8>, TransactionError> {
        if payload.is_empty() {
            return Ok(Vec::new());
        }
        let params = decode_params(payload)?;
        if !params.iter().any(|(field, _)| is_text_field(*field)) {
            return Ok(payload.to_vec());
        }

        let enabled = self.is_enabled();
        let should_xor = if enabled { true } else { detect_xor(&params) };

        if should_xor {
            let transformed = xor_params(&params);
            let encoded = encode_params(&transformed)?;
            if !enabled {
                self.enable();
            }
            Ok(encoded)
        } else {
            Ok(payload.to_vec())
        }
    }

    /// Encode a parameter payload, XOR-ing text fields when enabled.
    ///
    /// # Errors
    ///
    /// Returns an error if the parameter payload cannot be decoded or
    /// re-encoded.
    pub fn encode_payload(&self, payload: &[u8]) -> Result<Vec<u8>, TransactionError> {
        if payload.is_empty() || !self.is_enabled() {
            return Ok(payload.to_vec());
        }
        let params = decode_params(payload)?;
        if !params.iter().any(|(field, _)| is_text_field(*field)) {
            return Ok(payload.to_vec());
        }
        let transformed = xor_params(&params);
        encode_params(&transformed)
    }
}

fn detect_xor(params: &[(FieldId, Vec<u8>)]) -> bool {
    let mut saw_text = false;
    let mut all_valid = true;
    let mut all_xor_valid = true;

    for (field, data) in params {
        if !is_text_field(*field) {
            continue;
        }
        saw_text = true;
        if std::str::from_utf8(data).is_err() {
            all_valid = false;
        }
        let xor_bytes = xor_bytes(data);
        if std::str::from_utf8(&xor_bytes).is_err() {
            all_xor_valid = false;
        }
    }

    saw_text && !all_valid && all_xor_valid
}

fn xor_params(params: &[(FieldId, Vec<u8>)]) -> Vec<(FieldId, Vec<u8>)> {
    params
        .iter()
        .map(|(field, data)| {
            if is_text_field(*field) {
                (*field, xor_bytes(data))
            } else {
                (*field, data.clone())
            }
        })
        .collect()
}

fn xor_bytes(data: &[u8]) -> Vec<u8> { data.iter().map(|byte| byte ^ 0xff).collect() }

const fn is_text_field(field: FieldId) -> bool {
    matches!(
        field,
        FieldId::Login
            | FieldId::Password
            | FieldId::Data
            | FieldId::NewsCategory
            | FieldId::NewsArticle
            | FieldId::NewsPath
            | FieldId::NewsTitle
            | FieldId::NewsPoster
            | FieldId::NewsDataFlavor
            | FieldId::NewsArticleData
            | FieldId::FileName
    )
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::transaction::decode_params;

    fn build_payload(params: &[(FieldId, &[u8])]) -> Vec<u8> {
        encode_params(params).expect("payload encodes")
    }

    fn decode_param_map(payload: &[u8]) -> Vec<(FieldId, Vec<u8>)> {
        decode_params(payload).expect("payload decodes")
    }

    #[rstest]
    fn xor_bytes_round_trip() {
        let input = b"hello".to_vec();
        let encoded = xor_bytes(&input);
        let decoded = xor_bytes(&encoded);
        assert_eq!(decoded, input);
    }

    #[rstest]
    fn decode_payload_enables_xor_on_invalid_utf8() {
        let compat = XorCompatibility::disabled();
        let encoded_password = xor_bytes(b"secret");
        let encoded_login = xor_bytes(b"alice");
        let payload = build_payload(&[
            (FieldId::Login, encoded_login.as_slice()),
            (FieldId::Password, encoded_password.as_slice()),
        ]);

        let decoded = compat.decode_payload(&payload).expect("decode payload");
        let params = decode_param_map(&decoded);

        assert!(compat.is_enabled());
        assert_eq!(params[0].0, FieldId::Login);
        assert_eq!(params[0].1, b"alice");
        assert_eq!(params[1].0, FieldId::Password);
        assert_eq!(params[1].1, b"secret");
    }

    #[rstest]
    fn decode_payload_keeps_plaintext_when_valid() {
        let compat = XorCompatibility::disabled();
        let payload = build_payload(&[(FieldId::Password, b"secret")]);

        let decoded = compat.decode_payload(&payload).expect("decode payload");

        assert!(!compat.is_enabled());
        assert_eq!(decoded, payload);
    }

    #[rstest]
    fn encode_payload_xors_text_fields_when_enabled() {
        let compat = XorCompatibility::enabled();
        let payload = build_payload(&[
            (FieldId::Data, b"message"),
            (FieldId::NewsArticleId, 42i32.to_be_bytes().as_ref()),
        ]);

        let encoded = compat.encode_payload(&payload).expect("encode payload");
        let params = decode_param_map(&encoded);

        assert_eq!(params[0].0, FieldId::Data);
        assert_eq!(xor_bytes(&params[0].1), b"message");
        assert_eq!(params[1].0, FieldId::NewsArticleId);
        assert_eq!(params[1].1, 42i32.to_be_bytes());
    }

    #[rstest]
    fn encode_payload_noop_when_disabled() {
        let compat = XorCompatibility::disabled();
        let payload = build_payload(&[(FieldId::Password, b"secret")]);

        let encoded = compat.encode_payload(&payload).expect("encode payload");

        assert_eq!(encoded, payload);
    }
}
