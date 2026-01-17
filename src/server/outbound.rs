//! Outbound transport and messaging traits for the server boundary.
//!
//! These traits let domain code emit replies and notifications without
//! coupling to adapter-specific types. Concrete adapters (wireframe, legacy)
//! implement the traits to deliver frames over their respective transports.

use async_trait::async_trait;
use thiserror::Error;

use crate::transaction::Transaction;

/// Priority levels for outbound messaging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboundPriority {
    /// Time-sensitive messages that should be delivered ahead of low priority.
    High,
    /// Best-effort messages that yield to high priority traffic.
    Low,
}

/// Opaque identifier for an outbound connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OutboundConnectionId(u64);

impl OutboundConnectionId {
    /// Create a new outbound connection identifier.
    ///
    /// # Examples
    ///
    /// ```
    /// use mxd::server::outbound::OutboundConnectionId;
    ///
    /// let id = OutboundConnectionId::new(42);
    /// assert_eq!(id.as_u64(), 42);
    /// ```
    #[must_use]
    pub const fn new(value: u64) -> Self { Self(value) }

    /// Return the inner identifier value.
    ///
    /// # Examples
    ///
    /// ```
    /// use mxd::server::outbound::OutboundConnectionId;
    ///
    /// let id = OutboundConnectionId::new(7);
    /// assert_eq!(id.as_u64(), 7);
    /// ```
    #[must_use]
    pub const fn as_u64(self) -> u64 { self.0 }
}

impl From<u64> for OutboundConnectionId {
    fn from(value: u64) -> Self { Self(value) }
}

/// Target for outbound messaging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboundTarget {
    /// Send to the current connection.
    Current,
    /// Send to a specific connection identifier.
    Connection(OutboundConnectionId),
}

/// Errors returned by outbound transport or messaging adapters.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum OutboundError {
    /// The reply was already set for this request.
    #[error("reply already sent")]
    ReplyAlreadySent,
    /// The reply was never set by the handler.
    #[error("reply missing from outbound transport")]
    ReplyMissing,
    /// The target connection is not available.
    #[error("outbound target unavailable")]
    TargetUnavailable,
    /// Messaging is not available for this runtime.
    #[error("outbound messaging unavailable")]
    MessagingUnavailable,
    /// The outbound queue is full.
    #[error("outbound queue full")]
    QueueFull,
    /// The outbound queue has been closed.
    #[error("outbound queue closed")]
    QueueClosed,
}

/// Transport for sending a reply tied to the current request.
pub trait OutboundTransport: Send {
    /// Send the reply for the current request.
    ///
    /// # Errors
    ///
    /// Returns [`OutboundError::ReplyAlreadySent`] if a reply has already been
    /// recorded.
    fn send_reply(&mut self, reply: Transaction) -> Result<(), OutboundError>;
}

/// Messaging adapter for outbound notifications.
#[async_trait]
pub trait OutboundMessaging: Send + Sync {
    /// Push a message to a specific target.
    ///
    /// # Errors
    ///
    /// Returns [`OutboundError::TargetUnavailable`] if the target is missing,
    /// or a queue error if delivery fails.
    async fn push(
        &self,
        target: OutboundTarget,
        message: Transaction,
        priority: OutboundPriority,
    ) -> Result<(), OutboundError>;

    /// Broadcast a message to all known targets.
    ///
    /// # Errors
    ///
    /// Returns [`OutboundError::TargetUnavailable`] if no targets are
    /// available, or a queue error if delivery fails.
    async fn broadcast(
        &self,
        message: Transaction,
        priority: OutboundPriority,
    ) -> Result<(), OutboundError>;
}

/// In-memory reply buffer used by adapters that need to return a reply value.
#[derive(Debug, Default)]
pub struct ReplyBuffer {
    reply: Option<Transaction>,
}

impl ReplyBuffer {
    /// Create an empty reply buffer.
    #[must_use]
    pub const fn new() -> Self { Self { reply: None } }

    /// Take the buffered reply, if present.
    ///
    /// # Examples
    ///
    /// ```
    /// use mxd::{
    ///     server::outbound::{OutboundTransport, ReplyBuffer},
    ///     transaction::{FrameHeader, Transaction},
    /// };
    ///
    /// let mut buffer = ReplyBuffer::new();
    /// let tx = Transaction {
    ///     header: FrameHeader {
    ///         flags: 0,
    ///         is_reply: 1,
    ///         ty: 0,
    ///         id: 0,
    ///         error: 0,
    ///         total_size: 0,
    ///         data_size: 0,
    ///     },
    ///     payload: Vec::new(),
    /// };
    /// buffer.send_reply(tx).expect("reply stored");
    /// assert!(buffer.take_reply().is_some());
    /// assert!(buffer.take_reply().is_none());
    /// ```
    #[must_use]
    pub const fn take_reply(&mut self) -> Option<Transaction> { self.reply.take() }
}

impl OutboundTransport for ReplyBuffer {
    fn send_reply(&mut self, reply: Transaction) -> Result<(), OutboundError> {
        if self.reply.is_some() {
            return Err(OutboundError::ReplyAlreadySent);
        }
        self.reply = Some(reply);
        Ok(())
    }
}

/// Messaging adapter that reports an unavailable runtime.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopOutboundMessaging;

#[async_trait]
impl OutboundMessaging for NoopOutboundMessaging {
    async fn push(
        &self,
        _target: OutboundTarget,
        _message: Transaction,
        _priority: OutboundPriority,
    ) -> Result<(), OutboundError> {
        Err(OutboundError::MessagingUnavailable)
    }

    async fn broadcast(
        &self,
        _message: Transaction,
        _priority: OutboundPriority,
    ) -> Result<(), OutboundError> {
        Err(OutboundError::MessagingUnavailable)
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use tokio::runtime::Runtime;

    use super::*;
    use crate::transaction::FrameHeader;

    fn reply() -> Transaction {
        Transaction {
            header: FrameHeader {
                flags: 0,
                is_reply: 1,
                ty: 0,
                id: 1,
                error: 0,
                total_size: 0,
                data_size: 0,
            },
            payload: Vec::new(),
        }
    }

    #[rstest]
    fn reply_buffer_accepts_single_reply() {
        let mut buffer = ReplyBuffer::new();

        buffer.send_reply(reply()).expect("store reply");

        assert!(buffer.take_reply().is_some());
        assert!(buffer.take_reply().is_none());
    }

    #[rstest]
    fn reply_buffer_rejects_second_reply() {
        let mut buffer = ReplyBuffer::new();
        buffer.send_reply(reply()).expect("first reply");

        let err = buffer.send_reply(reply()).expect_err("second reply");

        assert_eq!(err, OutboundError::ReplyAlreadySent);
    }

    #[rstest]
    fn noop_messaging_reports_unavailable() {
        let rt = Runtime::new().expect("runtime");
        let messaging = NoopOutboundMessaging;

        let err = rt
            .block_on(messaging.push(OutboundTarget::Current, reply(), OutboundPriority::Low))
            .expect_err("push should fail");

        assert_eq!(err, OutboundError::MessagingUnavailable);
    }
}
