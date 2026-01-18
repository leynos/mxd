//! Reply builder for routing errors.
//!
//! Centralises error reply construction and logging so routing error paths
//! preserve transaction identifiers whenever possible and emit structured
//! tracing events.

use std::{fmt::Display, net::SocketAddr};

use tracing::{Level, error, warn};

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
        self.log_with_context(Level::WARN, Some(&err), error_code, message);
    }

    fn error_with_error<E: Display>(&self, err: E, error_code: u32, message: &'static str) {
        self.log_with_context(Level::ERROR, Some(&err), error_code, message);
    }

    fn error_without_error(&self, error_code: u32, message: &'static str) {
        self.log_with_context(Level::ERROR, None, error_code, message);
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "log inputs are intentionally explicit for call sites"
    )]
    fn log_with_context(
        &self,
        level: Level,
        error: Option<&dyn Display>,
        error_code: u32,
        message: &'static str,
    ) {
        let (ty, id) = header_fields(self.header.as_ref());
        let context = LogContext {
            ty,
            id,
            error_code,
            message,
        };
        match (level, error) {
            (Level::WARN, Some(err)) => self.log_warn_with_error(err, &context),
            (Level::ERROR, Some(err)) => self.log_error_with_error(err, &context),
            (Level::ERROR, None) => self.log_error_without_error(&context),
            #[expect(
                clippy::unreachable,
                reason = "unsupported log contexts are unreachable"
            )]
            _ => unreachable!("unsupported log context"),
        }
    }

    fn log_warn_with_error(&self, err: &dyn Display, context: &LogContext) {
        let LogContext {
            ty,
            id,
            error_code,
            message,
        } = *context;
        warn!(
            %err,
            peer = %self.peer,
            ty = ?ty,
            id = ?id,
            error_code,
            "{message}"
        );
    }

    fn log_error_with_error(&self, err: &dyn Display, context: &LogContext) {
        let LogContext {
            ty,
            id,
            error_code,
            message,
        } = *context;
        error!(
            %err,
            peer = %self.peer,
            ty = ?ty,
            id = ?id,
            error_code,
            "{message}"
        );
    }

    fn log_error_without_error(&self, context: &LogContext) {
        let LogContext {
            ty,
            id,
            error_code,
            message,
        } = *context;
        error!(
            peer = %self.peer,
            ty = ?ty,
            id = ?id,
            error_code,
            "{message}"
        );
    }
}

#[derive(Clone, Copy)]
struct LogContext {
    ty: Option<u16>,
    id: Option<u32>,
    error_code: u32,
    message: &'static str,
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

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        fmt,
        net::SocketAddr,
        sync::{Arc, Mutex},
    };

    use rstest::rstest;
    use tracing::{
        Event,
        Level,
        Metadata,
        Subscriber,
        field::{Field, Visit},
        span::{Attributes, Id, Record},
    };

    use super::ReplyBuilder;
    use crate::{transaction::FrameHeader, wireframe::test_helpers::transaction_bytes};

    #[derive(Clone, Default)]
    struct RecordingSubscriber {
        events: Arc<Mutex<Vec<RecordedEvent>>>,
    }

    impl RecordingSubscriber {
        fn take_events(&self) -> Vec<RecordedEvent> {
            std::mem::take(&mut *self.events.lock().expect("recording lock"))
        }
    }

    #[derive(Debug)]
    struct RecordedEvent {
        level: Level,
        fields: HashMap<String, String>,
        message: Option<String>,
    }

    #[derive(Default)]
    struct FieldRecorder {
        fields: HashMap<String, String>,
        message: Option<String>,
    }

    impl FieldRecorder {
        fn record_value(&mut self, field: &Field, value: String) {
            if field.name() == "message" {
                self.message = Some(value);
            } else {
                self.fields.insert(field.name().to_string(), value);
            }
        }
    }

    impl Visit for FieldRecorder {
        fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
            self.record_value(field, format!("{value:?}"));
        }

        fn record_i64(&mut self, field: &Field, value: i64) {
            self.record_value(field, value.to_string());
        }

        fn record_u64(&mut self, field: &Field, value: u64) {
            self.record_value(field, value.to_string());
        }

        fn record_bool(&mut self, field: &Field, value: bool) {
            self.record_value(field, value.to_string());
        }

        fn record_str(&mut self, field: &Field, value: &str) {
            self.record_value(field, value.to_string());
        }
    }

    impl Subscriber for RecordingSubscriber {
        fn enabled(&self, _metadata: &Metadata<'_>) -> bool { true }

        fn new_span(&self, _attrs: &Attributes<'_>) -> Id { Id::from_u64(1) }

        fn record(&self, _span: &Id, _values: &Record<'_>) {}

        fn record_follows_from(&self, _span: &Id, _follows: &Id) {}

        fn event(&self, event: &Event<'_>) {
            let mut recorder = FieldRecorder::default();
            event.record(&mut recorder);
            let record = RecordedEvent {
                level: *event.metadata().level(),
                fields: recorder.fields,
                message: recorder.message,
            };
            self.events.lock().expect("recording lock").push(record);
        }

        fn enter(&self, _span: &Id) {}

        fn exit(&self, _span: &Id) {}
    }

    #[rstest]
    fn parse_error_logs_transaction_context() {
        let peer: SocketAddr = "127.0.0.1:9000".parse().expect("peer");
        let header = FrameHeader {
            flags: 0,
            is_reply: 0,
            ty: 200,
            id: 42,
            error: 0,
            total_size: 0,
            data_size: 0,
        };
        let frame = transaction_bytes(&header, &[]);
        let subscriber = RecordingSubscriber::default();
        let dispatch = tracing::Dispatch::new(subscriber.clone());

        tracing::dispatcher::with_default(&dispatch, || {
            let builder = ReplyBuilder::from_frame(peer, &frame);
            let _ = builder.parse_error("parse fail", 3);
        });

        let events = subscriber.take_events();
        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert_eq!(event.level, Level::WARN);
        assert_eq!(
            event.fields.get("peer").map(String::as_str),
            Some("127.0.0.1:9000")
        );
        assert_eq!(
            event.fields.get("ty").map(String::as_str),
            Some("Some(200)")
        );
        assert_eq!(event.fields.get("id").map(String::as_str), Some("Some(42)"));
        assert_eq!(
            event.fields.get("error_code").map(String::as_str),
            Some("3")
        );
        assert_eq!(
            event.message.as_deref(),
            Some("failed to parse transaction from bytes")
        );
    }

    #[rstest]
    fn parse_error_logs_missing_header_as_none() {
        let peer: SocketAddr = "127.0.0.1:9001".parse().expect("peer");
        let subscriber = RecordingSubscriber::default();
        let dispatch = tracing::Dispatch::new(subscriber.clone());

        tracing::dispatcher::with_default(&dispatch, || {
            let builder = ReplyBuilder::from_frame(peer, &[]);
            let _ = builder.parse_error("short", 3);
        });

        let events = subscriber.take_events();
        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert_eq!(event.level, Level::WARN);
        assert_eq!(
            event.fields.get("peer").map(String::as_str),
            Some("127.0.0.1:9001")
        );
        assert_eq!(event.fields.get("ty").map(String::as_str), Some("None"));
        assert_eq!(event.fields.get("id").map(String::as_str), Some("None"));
        assert_eq!(
            event.fields.get("error_code").map(String::as_str),
            Some("3")
        );
    }
}
