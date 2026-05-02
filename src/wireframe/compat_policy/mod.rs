//! Compatibility policy derived from handshake and login metadata.
//!
//! The handshake sub-version identifies `SynHX` clients (sub-version 2). Hotline
//! 1.8.5 versus 1.9 is determined by the login request's version field (field
//! 160): `151..=189` maps to Hotline 1.8.5 and `>=190` maps to Hotline 1.9.
//! The adapter uses this policy to decide which login reply fields to include
//! without leaking quirks into the domain layer.

use std::sync::atomic::{AtomicU32, Ordering};

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

#[cfg(kani)]
mod kani;

const UNKNOWN_LOGIN_VERSION: u32 = u32::MAX;
const SYNHX_SUB_VERSION: u16 = 2;
const HOTLINE_85_MIN_VERSION: u16 = 151;
const HOTLINE_19_MIN_VERSION: u16 = 190;
const DEFAULT_BANNER_ID: i32 = 0;
const DEFAULT_SERVER_NAME: &str = "mxd";

/// Classification of clients for compatibility decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientKind {
    /// `SynHX` client identified by handshake sub-version 2.
    SynHx,
    /// Hotline 1.8.5 client (login version 151..=189).
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
    login_version: AtomicU32,
}

impl ClientCompatibility {
    /// Seed a compatibility policy from handshake metadata.
    #[must_use]
    pub const fn from_handshake(handshake: &HandshakeMetadata) -> Self {
        Self {
            handshake_sub_version: handshake.sub_version,
            login_version: AtomicU32::new(UNKNOWN_LOGIN_VERSION),
        }
    }

    /// Record the client version observed in the login request.
    pub fn record_login_version(&self, version: u16) {
        self.login_version
            .store(u32::from(version), Ordering::Relaxed);
    }

    /// Return the recorded login version, if available.
    #[must_use]
    pub fn login_version(&self) -> Option<u16> {
        let version = self.login_version.load(Ordering::Relaxed);
        (version != UNKNOWN_LOGIN_VERSION)
            .then_some(version)
            .and_then(|raw| u16::try_from(raw).ok())
    }

    /// Classify the connection by known handshake and login metadata.
    #[must_use]
    pub fn kind(&self) -> ClientKind {
        if self.handshake_sub_version == SYNHX_SUB_VERSION {
            return ClientKind::SynHx;
        }
        match self.login_version() {
            Some(version) if version >= HOTLINE_19_MIN_VERSION => ClientKind::Hotline19,
            Some(version) if version >= HOTLINE_85_MIN_VERSION => ClientKind::Hotline85,
            Some(_) | None => ClientKind::Unknown,
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
#[path = "../compat_policy_tests.rs"]
mod tests;
