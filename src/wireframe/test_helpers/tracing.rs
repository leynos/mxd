//! Tracing test helpers for wireframe tests.
//!
//! Provides a tiny capture harness so tests can assert structured fields from
//! `tracing` events without adding external dependencies.

use std::{
    collections::HashMap,
    fmt,
    sync::{Arc, Mutex},
};

use tracing::{
    Event,
    Level,
    Metadata,
    Subscriber,
    field::{Field, Visit},
    span::{Attributes, Id, Record},
};

#[derive(Clone, Default)]
struct RecordingSubscriber {
    events: Arc<Mutex<Vec<RecordedEvent>>>,
}

impl RecordingSubscriber {
    fn take_events(&self) -> Vec<RecordedEvent> {
        let mut guard = match self.events.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        std::mem::take(&mut *guard)
    }
}

/// Captured tracing event fields for wireframe tests.
///
/// # Examples
/// ```ignore
/// use mxd::wireframe::test_helpers::tracing::capture_single_event;
///
/// let event = capture_single_event(|| tracing::warn!(error_code = 1, "oops"));
/// assert_eq!(event.field("error_code"), Some("1"));
/// ```
#[derive(Debug)]
pub(crate) struct RecordedEvent {
    level: Level,
    fields: HashMap<String, String>,
    message: Option<String>,
}

impl RecordedEvent {
    /// Return the captured event level.
    ///
    /// # Examples
    /// ```ignore
    /// use tracing::Level;
    /// use mxd::wireframe::test_helpers::tracing::capture_single_event;
    ///
    /// let event = capture_single_event(|| tracing::warn!("oops"));
    /// assert_eq!(event.level(), Level::WARN);
    /// ```
    pub(crate) const fn level(&self) -> Level { self.level }

    /// Return a captured field value by name.
    ///
    /// # Examples
    /// ```ignore
    /// use mxd::wireframe::test_helpers::tracing::capture_single_event;
    ///
    /// let event = capture_single_event(|| tracing::warn!(error_code = 7, "oops"));
    /// assert_eq!(event.field("error_code"), Some("7"));
    /// ```
    pub(crate) fn field(&self, name: &str) -> Option<&str> {
        self.fields.get(name).map(String::as_str)
    }

    /// Return the captured message, if present.
    ///
    /// # Examples
    /// ```ignore
    /// use mxd::wireframe::test_helpers::tracing::capture_single_event;
    ///
    /// let event = capture_single_event(|| tracing::warn!("oops"));
    /// assert_eq!(event.message(), Some("oops"));
    /// ```
    pub(crate) fn message(&self) -> Option<&str> { self.message.as_deref() }
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
            self.fields.insert(field.name().to_owned(), value);
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
        self.record_value(field, value.to_owned());
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
        let mut guard = match self.events.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.push(record);
    }

    fn enter(&self, _span: &Id) {}

    fn exit(&self, _span: &Id) {}
}

/// Capture a single `tracing` event emitted by the provided closure.
///
/// # Examples
/// ```ignore
/// use tracing::Level;
/// use mxd::wireframe::test_helpers::tracing::capture_single_event;
///
/// let event = capture_single_event(|| tracing::warn!(error_code = 1, "oops"));
/// assert_eq!(event.level(), Level::WARN);
/// ```
pub(crate) fn capture_single_event(f: impl FnOnce()) -> RecordedEvent {
    let subscriber = RecordingSubscriber::default();
    let dispatch = tracing::Dispatch::new(subscriber.clone());

    tracing::dispatcher::with_default(&dispatch, f);

    let mut events = subscriber.take_events();
    assert_eq!(events.len(), 1, "expected exactly one tracing event");
    events.remove(0)
}
