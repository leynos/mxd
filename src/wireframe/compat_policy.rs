//! Compatibility policy derived from handshake and login metadata.
//!
//! The handshake sub-version identifies `SynHX` clients (sub-version 2). Hotline
//! 1.8.5 versus 1.9 is determined by the login request's version field (field
//! 160). The adapter uses this policy to decide which login reply fields to
//! include without leaking quirks into the domain layer.

use std::sync::atomic::{AtomicU16, Ordering};

use crate::{
    field_id::FieldId,
    transaction::{
        Transaction,
        TransactionError,
        decode_params,
        encode_params,
        read_u16,
        read_u32,
    },
    transaction_type::TransactionType,
    wireframe::connection::HandshakeMetadata,
};

const UNKNOWN_LOGIN_VERSION: u16 = u16::MAX;
const SYNHX_SUB_VERSION: u16 = 2;
const HOTLINE_19_MIN_VERSION: u16 = 190;
const DEFAULT_BANNER_ID: i32 = 0;
const DEFAULT_SERVER_NAME: &str = "mxd";

/// Classification of clients for compatibility decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientKind {
    /// `SynHX` client identified by handshake sub-version 2.
    SynHx,
    /// Hotline 1.8.5 client (login version < 190).
    Hotline85,
    /// Hotline 1.9 client (login version >= 190).
    Hotline19,
    /// Unknown client type; login version not observed yet.
    Unknown,
}

/// Mutable compatibility state for a single connection.
#[derive(Debug)]
pub struct ClientCompatibility {
    handshake_sub_version: u16,
    login_version: AtomicU16,
}

impl ClientCompatibility {
    /// Seed a compatibility policy from handshake metadata.
    #[must_use]
    pub const fn from_handshake(handshake: &HandshakeMetadata) -> Self {
        Self {
            handshake_sub_version: handshake.sub_version,
            login_version: AtomicU16::new(UNKNOWN_LOGIN_VERSION),
        }
    }

    /// Record the client version observed in the login request.
    pub fn record_login_version(&self, version: u16) {
        self.login_version.store(version, Ordering::Relaxed);
    }

    /// Return the recorded login version, if available.
    #[must_use]
    pub fn login_version(&self) -> Option<u16> {
        let version = self.login_version.load(Ordering::Relaxed);
        if version == UNKNOWN_LOGIN_VERSION {
            None
        } else {
            Some(version)
        }
    }

    /// Classify the connection by known handshake and login metadata.
    #[must_use]
    pub fn kind(&self) -> ClientKind {
        if self.handshake_sub_version == SYNHX_SUB_VERSION {
            return ClientKind::SynHx;
        }
        match self.login_version() {
            Some(version) if version >= HOTLINE_19_MIN_VERSION => ClientKind::Hotline19,
            Some(_) => ClientKind::Hotline85,
            None => ClientKind::Unknown,
        }
    }

    /// Returns true when the login reply should include banner fields 161/162.
    #[must_use]
    pub fn should_include_login_extras(&self) -> bool {
        matches!(self.kind(), ClientKind::Hotline85 | ClientKind::Hotline19)
    }

    /// Capture the login version from the login request payload.
    ///
    /// # Errors
    ///
    /// Returns a [`TransactionError`] when the payload cannot be decoded.
    pub fn record_login_payload(&self, payload: &[u8]) -> Result<(), TransactionError> {
        if let Some(version) = extract_login_version(payload)? {
            self.record_login_version(version);
        }
        Ok(())
    }

    /// Augment a successful login reply with banner fields when required.
    ///
    /// Returns `true` if the reply was modified.
    ///
    /// # Errors
    ///
    /// Returns a [`TransactionError`] if the reply payload cannot be decoded or
    /// re-encoded.
    pub fn augment_login_reply(&self, reply: &mut Transaction) -> Result<bool, TransactionError> {
        if reply.header.error != 0
            || TransactionType::from(reply.header.ty) != TransactionType::Login
        {
            return Ok(false);
        }
        if !self.should_include_login_extras() {
            return Ok(false);
        }
        let mut params = decode_params(&reply.payload)?;
        let has_banner_id = params.iter().any(|(id, _)| *id == FieldId::BannerId);
        let has_server_name = params.iter().any(|(id, _)| *id == FieldId::ServerName);

        if !has_banner_id {
            #[expect(clippy::big_endian_bytes, reason = "network protocol uses big-endian")]
            let banner_id_bytes = DEFAULT_BANNER_ID.to_be_bytes();
            params.push((FieldId::BannerId, banner_id_bytes.to_vec()));
        }
        if !has_server_name {
            params.push((FieldId::ServerName, DEFAULT_SERVER_NAME.as_bytes().to_vec()));
        }

        if has_banner_id && has_server_name {
            return Ok(false);
        }

        reply.payload = encode_params(&params)?;
        let payload_len =
            u32::try_from(reply.payload.len()).map_err(|_| TransactionError::PayloadTooLarge)?;
        reply.header.total_size = payload_len;
        reply.header.data_size = payload_len;
        Ok(true)
    }
}

fn extract_login_version(payload: &[u8]) -> Result<Option<u16>, TransactionError> {
    let params = decode_params(payload)?;
    for (field, data) in params {
        if field == FieldId::Version {
            return Ok(parse_login_version(&data));
        }
    }
    Ok(None)
}

fn parse_login_version(data: &[u8]) -> Option<u16> {
    match data.len() {
        2 => read_u16(data).ok(),
        4 => read_u32(data).ok().and_then(|raw| u16::try_from(raw).ok()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::big_endian_bytes,
        reason = "test vectors validate network-endian encoding"
    )]

    use rstest::rstest;

    use super::*;
    use crate::{
        protocol::VERSION,
        transaction::{FrameHeader, Transaction, encode_params},
    };

    fn handshake(sub_version: u16) -> HandshakeMetadata {
        HandshakeMetadata {
            sub_protocol: u32::from_be_bytes(*b"HOTL"),
            version: VERSION,
            sub_version,
        }
    }

    fn reply_header(payload_len: usize) -> FrameHeader {
        let payload_len = u32::try_from(payload_len).expect("payload length fits in u32");
        FrameHeader {
            flags: 0,
            is_reply: 1,
            ty: TransactionType::Login.into(),
            id: 1,
            error: 0,
            total_size: payload_len,
            data_size: payload_len,
        }
    }

    #[rstest]
    fn classifies_synhx_from_handshake() {
        let compat = ClientCompatibility::from_handshake(&handshake(SYNHX_SUB_VERSION));
        assert_eq!(compat.kind(), ClientKind::SynHx);
    }

    #[rstest]
    fn classifies_hotline_85_from_login_version() {
        let compat = ClientCompatibility::from_handshake(&handshake(0));
        compat.record_login_version(151);
        assert_eq!(compat.kind(), ClientKind::Hotline85);
    }

    #[rstest]
    fn classifies_hotline_19_from_login_version() {
        let compat = ClientCompatibility::from_handshake(&handshake(0));
        compat.record_login_version(190);
        assert_eq!(compat.kind(), ClientKind::Hotline19);
    }

    #[rstest]
    fn records_login_version_from_u16_payload() {
        let compat = ClientCompatibility::from_handshake(&handshake(0));
        let payload =
            encode_params(&[(FieldId::Version, 151u16.to_be_bytes())]).expect("payload encodes");

        compat
            .record_login_payload(&payload)
            .expect("record login version");

        assert_eq!(compat.login_version(), Some(151));
    }

    #[rstest]
    fn records_login_version_from_u32_payload() {
        let compat = ClientCompatibility::from_handshake(&handshake(0));
        let payload =
            encode_params(&[(FieldId::Version, 190u32.to_be_bytes())]).expect("payload encodes");

        compat
            .record_login_payload(&payload)
            .expect("record login version");

        assert_eq!(compat.login_version(), Some(190));
    }

    fn assert_login_reply_augmentation(
        sub_version: u16,
        login_version: u16,
        expected_updated: bool,
    ) {
        let compat = ClientCompatibility::from_handshake(&handshake(sub_version));
        compat.record_login_version(login_version);
        let payload = encode_params(&[(FieldId::Version, login_version.to_be_bytes())])
            .expect("payload encodes");
        let header = reply_header(payload.len());
        let mut reply = Transaction { header, payload };

        let updated = compat
            .augment_login_reply(&mut reply)
            .expect("augment reply");

        assert_eq!(updated, expected_updated);
        let params = decode_params(&reply.payload).expect("decode reply params");
        let has_banner_id = params.iter().any(|(id, _)| *id == FieldId::BannerId);
        let has_server_name = params.iter().any(|(id, _)| *id == FieldId::ServerName);
        assert_eq!(has_banner_id, expected_updated);
        assert_eq!(has_server_name, expected_updated);
    }

    #[rstest]
    fn augments_login_reply_when_required() { assert_login_reply_augmentation(0, 151, true); }

    #[rstest]
    fn does_not_augment_login_reply_for_synhx() {
        assert_login_reply_augmentation(SYNHX_SUB_VERSION, 190, false);
    }
}
