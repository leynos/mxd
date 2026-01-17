//! Reply builder for routing errors.
//!
//! Centralises error reply construction and logging so routing error paths
//! preserve transaction identifiers whenever possible and emit structured
//! tracing events.

use std::{fmt::Display, net::SocketAddr};

use tracing::{error, warn};

use crate::{
    header_util::reply_header,
    transaction::{FrameHeader, HEADER_LEN, Transaction},
};

#[derive(Debug, Clone)]
pub(super) struct ReplyBuilder {
    peer: SocketAddr,
    header: Option<FrameHeader>,
}

impl ReplyBuilder {
    pub(super) fn from_frame(peer: SocketAddr, frame: &[u8]) -> Self {
        let header = frame
            .get(..HEADER_LEN)
            .and_then(|slice| slice.try_into().ok())
            .map(FrameHeader::from_bytes);
        Self { peer, header }
    }

    pub(super) const fn from_header(peer: SocketAddr, header: FrameHeader) -> Self {
        Self {
            peer,
            header: Some(header),
        }
    }

    pub(super) fn parse_error<E: Display>(&self, err: E, error_code: u32) -> Vec<u8> {
        self.warn_with_error(err, error_code, "failed to parse transaction from bytes");
        self.error_bytes(error_code)
    }

    pub(super) fn command_parse_error<E: Display>(&self, err: E, error_code: u32) -> Vec<u8> {
        self.warn_with_error(err, error_code, "failed to parse command from transaction");
        self.error_bytes(error_code)
    }

    pub(super) fn process_error<E: Display>(&self, err: E, error_code: u32) -> Vec<u8> {
        self.error_with_error(err, error_code, "command processing failed");
        self.error_bytes(error_code)
    }

    pub(super) fn missing_reply(&self, error_code: u32) -> Vec<u8> {
        self.error_without_error(error_code, "command processing did not emit a reply");
        self.error_bytes(error_code)
    }

    pub(super) fn error_transaction(&self, error_code: u32) -> Transaction {
        let request_header = self.header.clone().unwrap_or_else(default_header);
        Transaction {
            header: reply_header(&request_header, error_code, 0),
            payload: Vec::new(),
        }
    }

    fn error_bytes(&self, error_code: u32) -> Vec<u8> {
        self.error_transaction(error_code).to_bytes()
    }

    fn warn_with_error<E: Display>(&self, err: E, error_code: u32, message: &'static str) {
        let (ty, id) = header_fields(self.header.as_ref());
        warn!(
            %err,
            %self.peer,
            ty = ?ty,
            id = ?id,
            error_code,
            "{message}"
        );
    }

    fn error_with_error<E: Display>(&self, err: E, error_code: u32, message: &'static str) {
        let (ty, id) = header_fields(self.header.as_ref());
        error!(
            %err,
            %self.peer,
            ty = ?ty,
            id = ?id,
            error_code,
            "{message}"
        );
    }

    fn error_without_error(&self, error_code: u32, message: &'static str) {
        let (ty, id) = header_fields(self.header.as_ref());
        error!(
            %self.peer,
            ty = ?ty,
            id = ?id,
            error_code,
            "{message}"
        );
    }
}

fn header_fields(header: Option<&FrameHeader>) -> (Option<u16>, Option<u32>) {
    header.map_or((None, None), |hdr| (Some(hdr.ty), Some(hdr.id)))
}

const fn default_header() -> FrameHeader {
    FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: 0,
        id: 0,
        error: 0,
        total_size: 0,
        data_size: 0,
    }
}
