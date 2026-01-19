//! Reply builder for routing errors.
//!
//! Centralizes error reply construction and logging so routing error paths
//! preserve transaction identifiers whenever possible and emit structured
//! tracing events.

use std::{fmt::Display, net::SocketAddr};

use crate::{
    header_util::reply_header,
    transaction::{FrameHeader, HEADER_LEN, Transaction},
};

#[derive(Debug, Clone)]
pub(super) struct ReplyBuilder {
    peer: SocketAddr,
    header: Option<FrameHeader>,
}

macro_rules! log_method {
    ($name:ident, $level:ident, with_error) => {
        fn $name<E: Display>(&self, err: E, error_code: u32, message: &'static str) {
            let (ty, id) = self.header_ids();
            tracing::$level!(
                %err,
                peer = %self.peer,
                ty = ?ty,
                id = ?id,
                error_code,
                "{message}"
            );
        }
    };
    ($name:ident, $level:ident, without_error) => {
        fn $name(&self, error_code: u32, message: &'static str) {
            let (ty, id) = self.header_ids();
            tracing::$level!(
                peer = %self.peer,
                ty = ?ty,
                id = ?id,
                error_code,
                "{message}"
            );
        }
    };
}

impl ReplyBuilder {
    pub(super) fn from_frame(peer: SocketAddr, frame: &[u8]) -> Self {
        let header = frame
            .get(..HEADER_LEN)
            .and_then(|slice| slice.try_into().ok())
            .map(FrameHeader::from_bytes);
        Self { peer, header }
    }

    pub(super) fn from_header(peer: SocketAddr, header: &FrameHeader) -> Self {
        Self {
            peer,
            header: Some(header.clone()),
        }
    }

    pub(super) fn parse_error<E: Display>(&self, err: E, error_code: u32) -> Vec<u8> {
        self.log_warn_with_error(err, error_code, "failed to parse transaction from bytes");
        self.error_bytes(error_code)
    }

    pub(super) fn command_parse_error<E: Display>(&self, err: E, error_code: u32) -> Vec<u8> {
        self.log_warn_with_error(err, error_code, "failed to parse command from transaction");
        self.error_bytes(error_code)
    }

    pub(super) fn process_error<E: Display>(&self, err: E, error_code: u32) -> Vec<u8> {
        self.log_error_with_error(err, error_code, "command processing failed");
        self.error_bytes(error_code)
    }

    pub(super) fn missing_reply(&self, error_code: u32) -> Vec<u8> {
        self.log_error_without_error(error_code, "command processing did not emit a reply");
        self.error_bytes(error_code)
    }

    pub(super) fn error_transaction(&self, error_code: u32) -> Transaction {
        let request_header = self.request_header_or_default();
        Transaction {
            header: reply_header(&request_header, error_code, 0),
            payload: Vec::new(),
        }
    }

    fn error_bytes(&self, error_code: u32) -> Vec<u8> {
        self.error_transaction(error_code).to_bytes()
    }

    log_method!(log_warn_with_error, warn, with_error);
    log_method!(log_error_with_error, error, with_error);
    log_method!(log_error_without_error, error, without_error);

    fn header_ids(&self) -> (Option<u16>, Option<u32>) {
        self.header
            .as_ref()
            .map_or((None, None), |hdr| (Some(hdr.ty), Some(hdr.id)))
    }

    fn request_header_or_default(&self) -> FrameHeader {
        self.header.clone().unwrap_or(FrameHeader {
            flags: 0,
            is_reply: 0,
            ty: 0,
            id: 0,
            error: 0,
            total_size: 0,
            data_size: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use rstest::rstest;
    use tracing::Level;

    use super::ReplyBuilder;
    use crate::{
        transaction::FrameHeader,
        wireframe::test_helpers::{
            tracing::{RecordedEvent, capture_single_event},
            transaction_bytes,
        },
    };

    struct ExpectedEvent<'a> {
        level: Level,
        peer: &'a str,
        ty: Option<u16>,
        id: Option<u32>,
        error_code: u32,
        message: Option<&'a str>,
        err: Option<&'a str>,
    }

    fn capture_and_assert_event<F>(action: F, expected: &ExpectedEvent<'_>)
    where
        F: FnOnce(),
    {
        let event = capture_single_event(action);
        assert_event_fields(&event, expected);
    }

    fn assert_event_fields(event: &RecordedEvent, expected: &ExpectedEvent<'_>) {
        assert_event_metadata(event, expected);
        assert_event_message(event, expected.message);
        assert_event_error(event, expected.err);
    }

    fn assert_event_metadata(event: &RecordedEvent, expected: &ExpectedEvent<'_>) {
        let expected_ty = format!("{:?}", expected.ty);
        let expected_id = format!("{:?}", expected.id);
        let expected_error_code = expected.error_code.to_string();

        assert_eq!(event.level(), expected.level);
        assert_field_value(event, "peer", expected.peer);
        assert_field_value(event, "ty", expected_ty.as_str());
        assert_field_value(event, "id", expected_id.as_str());
        assert_field_value(event, "error_code", expected_error_code.as_str());
    }

    fn assert_event_message(event: &RecordedEvent, expected: Option<&str>) {
        match expected {
            Some(message) => assert_eq!(event.message(), Some(message)),
            None => assert!(event.message().is_none(), "expected no message field"),
        }
    }

    fn assert_event_error(event: &RecordedEvent, expected: Option<&str>) {
        match expected {
            Some(err) => {
                let value = event.field("err").expect("expected err field");
                assert_eq!(value.trim_matches('"'), err);
            }
            None => assert!(event.field("err").is_none(), "expected no err field"),
        }
    }

    fn assert_field_value(event: &RecordedEvent, field: &str, expected: &str) {
        assert_eq!(event.field(field), Some(expected));
    }

    fn test_header(ty: u16, id: u32) -> FrameHeader {
        FrameHeader {
            flags: 0,
            is_reply: 0,
            ty,
            id,
            error: 0,
            total_size: 0,
            data_size: 0,
        }
    }

    fn capture_and_assert_with_header<F>(
        peer: SocketAddr,
        header: &FrameHeader,
        expected: &ExpectedEvent<'_>,
        action: F,
    ) where
        F: FnOnce(SocketAddr, &FrameHeader),
    {
        capture_and_assert_event(|| action(peer, header), expected);
    }

    #[rstest]
    fn parse_error_logs_transaction_context() {
        let peer: SocketAddr = "127.0.0.1:9000".parse().expect("peer");
        let header = test_header(200, 42);
        capture_and_assert_with_header(
            peer,
            &header,
            &ExpectedEvent {
                level: Level::WARN,
                peer: "127.0.0.1:9000",
                ty: Some(200),
                id: Some(42),
                error_code: 3,
                message: Some("failed to parse transaction from bytes"),
                err: Some("parse fail"),
            },
            |peer, header| {
                let frame = transaction_bytes(header, &[]);
                let builder = ReplyBuilder::from_frame(peer, &frame);
                let _ = builder.parse_error("parse fail", 3);
            },
        );
    }

    #[rstest]
    fn parse_error_logs_missing_header_as_none() {
        let peer: SocketAddr = "127.0.0.1:9001".parse().expect("peer");
        capture_and_assert_event(
            || {
                let builder = ReplyBuilder::from_frame(peer, &[]);
                let _ = builder.parse_error("short", 3);
            },
            &ExpectedEvent {
                level: Level::WARN,
                peer: "127.0.0.1:9001",
                ty: None,
                id: None,
                error_code: 3,
                message: Some("failed to parse transaction from bytes"),
                err: Some("short"),
            },
        );
    }

    #[rstest]
    fn missing_reply_logs_without_error_field() {
        let peer: SocketAddr = "127.0.0.1:9002".parse().expect("peer");
        let header = test_header(7, 99);
        capture_and_assert_with_header(
            peer,
            &header,
            &ExpectedEvent {
                level: Level::ERROR,
                peer: "127.0.0.1:9002",
                ty: Some(7),
                id: Some(99),
                error_code: 5,
                message: Some("command processing did not emit a reply"),
                err: None,
            },
            |peer, header| {
                let builder = ReplyBuilder::from_header(peer, header);
                let _ = builder.missing_reply(5);
            },
        );
    }
}
