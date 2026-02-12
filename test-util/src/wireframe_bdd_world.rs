//! Shared world state and helpers for wireframe BDD tests.
//!
//! This world launches the `mxd-wireframe-server` binary and interacts with it
//! through a real TCP connection so behavioural tests exercise transport and
//! routing together.

use std::{
    cell::{Cell, RefCell},
    io::{Read, Write},
    net::TcpStream,
    time::Duration,
};

use anyhow::Context as _;
use mxd::{
    commands::{ERR_INTERNAL_SERVER, ERR_NOT_AUTHENTICATED},
    field_id::FieldId,
    transaction::{FrameHeader, HEADER_LEN, MAX_FRAME_DATA, MAX_PAYLOAD_SIZE, Transaction},
    transaction_type::TransactionType,
    wireframe::{connection::HandshakeMetadata, test_helpers::build_frame},
};

#[cfg(feature = "postgres")]
use crate::postgres::PostgresTestDbError;
use crate::{AnyError, DatabaseUrl, SetupFn, TestServer, protocol::handshake_with_sub_version};

const DEFAULT_IO_TIMEOUT: Duration = Duration::from_secs(10);
const IO_TIMEOUT_ENV_VAR: &str = "TEST_IO_TIMEOUT_SECS";

/// Shared BDD world backing for binary transport scenarios.
pub struct WireframeBddWorld {
    server: RefCell<Option<TestServer>>,
    stream: RefCell<Option<TcpStream>>,
    reply: RefCell<Option<Result<Transaction, String>>>,
    io_timeout: Cell<Duration>,
    handshake_sub_version: Cell<u16>,
    skipped: Cell<bool>,
}

impl Default for WireframeBddWorld {
    fn default() -> Self { Self::new() }
}

impl WireframeBddWorld {
    /// Create a fresh wireframe BDD world.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            server: RefCell::new(None),
            stream: RefCell::new(None),
            reply: RefCell::new(None),
            io_timeout: Cell::new(DEFAULT_IO_TIMEOUT),
            handshake_sub_version: Cell::new(0),
            skipped: Cell::new(false),
        }
    }

    /// Return true when backend availability caused this scenario to be skipped.
    #[must_use]
    pub const fn is_skipped(&self) -> bool { self.skipped.get() }

    /// Override socket read and write timeout for this world.
    ///
    /// This can be used by slower integration environments that need longer
    /// deadlines than the default ten seconds.
    ///
    /// When `TEST_IO_TIMEOUT_SECS` is set, the environment value takes
    /// precedence over this field value.
    pub fn set_io_timeout(&self, timeout: Duration) { self.io_timeout.set(timeout); }

    /// Build and install a fixture database for this scenario, then connect.
    ///
    /// # Errors
    ///
    /// Returns an error when fixture setup, server startup, or client
    /// connection/handshake fails.
    pub fn setup_db(&self, setup: SetupFn) -> Result<(), AnyError> {
        if self.is_skipped() {
            return Ok(());
        }
        self.reply.borrow_mut().take();

        let server =
            match TestServer::start_with_setup("./Cargo.toml", |db| setup(DatabaseUrl::from(db))) {
                Ok(server) => server,
                Err(error) => {
                    #[cfg(feature = "postgres")]
                    if error
                        .downcast_ref::<PostgresTestDbError>()
                        .is_some_and(PostgresTestDbError::is_unavailable)
                    {
                        self.skipped.set(true);
                        return Ok(());
                    }
                    return Err(error).context("failed to start wireframe test server");
                }
            };

        self.server.borrow_mut().replace(server);
        self.reconnect()
            .context("failed to connect to wireframe test server")?;
        Ok(())
    }

    /// Update handshake compatibility metadata for this connection.
    ///
    /// When the test client is already connected, this reconnects so the next
    /// request uses the updated handshake values.
    pub fn set_client_compat_from_handshake(&self, handshake: &HandshakeMetadata) {
        self.handshake_sub_version.set(handshake.sub_version);
        if self.is_skipped() || self.server.borrow().is_none() {
            return;
        }
        if let Err(error) = self.reconnect() {
            self.set_reply_error(format!(
                "failed to reconnect with handshake sub-version {}: {error}",
                handshake.sub_version
            ));
        }
    }

    fn reconnect(&self) -> Result<(), AnyError> {
        let server_addr = self
            .server
            .borrow()
            .as_ref()
            .map(TestServer::bind_addr)
            .ok_or_else(|| anyhow::anyhow!("wireframe test server has not been started"))?;

        let io_timeout = self.io_timeout();
        let mut stream = TcpStream::connect(server_addr)?;
        stream.set_read_timeout(Some(io_timeout))?;
        stream.set_write_timeout(Some(io_timeout))?;
        handshake_with_sub_version(&mut stream, self.handshake_sub_version.get())?;
        self.stream.borrow_mut().replace(stream);
        Ok(())
    }

    fn io_timeout(&self) -> Duration {
        Self::io_timeout_from_env().unwrap_or_else(|| self.io_timeout.get())
    }

    fn io_timeout_from_env() -> Option<Duration> {
        std::env::var(IO_TIMEOUT_ENV_VAR)
            .ok()?
            .parse::<u64>()
            .ok()
            .map(Duration::from_secs)
    }

    /// Validate continuation frame header consistency with the original header.
    const fn is_valid_continuation(original: &FrameHeader, continuation: &FrameHeader) -> bool {
        continuation.flags == original.flags
            && continuation.is_reply == original.is_reply
            && continuation.ty == original.ty
            && continuation.id == original.id
            && continuation.error == original.error
            && continuation.total_size == original.total_size
    }

    fn read_reply(stream: &mut TcpStream) -> Result<Transaction, AnyError> {
        let mut header_buf = [0u8; HEADER_LEN];
        stream.read_exact(&mut header_buf)?;
        let mut header = FrameHeader::from_bytes(&header_buf);
        let (total_size, first_data_size) = Self::validate_initial_header(&header)?;

        let mut payload = vec![0u8; first_data_size];
        if first_data_size > 0 {
            stream.read_exact(&mut payload)?;
        }

        while payload.len() < total_size {
            let remaining = total_size - payload.len();
            let continuation_payload = Self::read_continuation_frame(stream, &header, remaining)?;
            payload.extend_from_slice(&continuation_payload);
        }

        header.data_size = header.total_size;
        Ok(Transaction { header, payload })
    }

    fn validate_initial_header(header: &FrameHeader) -> Result<(usize, usize), AnyError> {
        let total_size = header.total_size as usize;
        let first_data_size = header.data_size as usize;
        if total_size > MAX_PAYLOAD_SIZE {
            return Err(anyhow::anyhow!(
                "reply total payload exceeds limit: {total_size} > {MAX_PAYLOAD_SIZE}",
            ));
        }
        if first_data_size > MAX_FRAME_DATA {
            return Err(anyhow::anyhow!(
                "reply frame payload exceeds frame limit: {first_data_size} > {MAX_FRAME_DATA}",
            ));
        }
        if first_data_size > total_size {
            return Err(anyhow::anyhow!(
                "reply data size exceeds total size: {first_data_size} > {total_size}",
            ));
        }
        if total_size > 0 && first_data_size == 0 {
            return Err(anyhow::anyhow!(
                "reply has non-zero total size but zero first frame size",
            ));
        }
        Ok((total_size, first_data_size))
    }

    fn read_continuation_frame(
        stream: &mut TcpStream,
        header: &FrameHeader,
        remaining: usize,
    ) -> Result<Vec<u8>, AnyError> {
        let mut continuation_header_buf = [0u8; HEADER_LEN];
        stream.read_exact(&mut continuation_header_buf)?;
        let continuation_header = FrameHeader::from_bytes(&continuation_header_buf);
        if !Self::is_valid_continuation(header, &continuation_header) {
            return Err(anyhow::anyhow!("reply continuation header mismatch"));
        }

        let continuation_size = continuation_header.data_size as usize;
        if continuation_size == 0 {
            return Err(anyhow::anyhow!(
                "reply continuation frame had zero payload size"
            ));
        }
        if continuation_size > MAX_FRAME_DATA {
            return Err(anyhow::anyhow!(
                "reply continuation payload exceeds frame limit: {continuation_size} > \
                 {MAX_FRAME_DATA}",
            ));
        }
        if continuation_size > remaining {
            return Err(anyhow::anyhow!(
                "reply continuation payload exceeds remaining bytes: {continuation_size} > \
                 {remaining}",
            ));
        }

        let mut continuation_payload = vec![0u8; continuation_size];
        stream.read_exact(&mut continuation_payload)?;
        Ok(continuation_payload)
    }

    fn send_frame(&self, frame: &[u8]) -> Result<Transaction, AnyError> {
        let mut stream_ref = self.stream.borrow_mut();
        let stream = stream_ref
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("wireframe test stream has not been connected"))?;
        stream.write_all(frame)?;
        Self::read_reply(stream)
    }

    fn send_login_with_credentials(
        &self,
        username: &[u8],
        password: &[u8],
    ) -> Result<Transaction, String> {
        let frame = build_frame(
            TransactionType::Login,
            90,
            &[(FieldId::Login, username), (FieldId::Password, password)],
        )
        .map_err(|error| format!("failed to build login frame: {error}"))?;
        self.send_frame(&frame).map_err(|error| error.to_string())
    }

    /// Route a raw frame through the running wireframe server binary.
    pub fn send_raw(&self, frame: &[u8]) {
        if self.is_skipped() {
            return;
        }
        let outcome = self.send_frame(frame).map_err(|error| error.to_string());
        self.reply.borrow_mut().replace(outcome);
    }

    /// Store a pre-routing failure as the scenario reply outcome.
    pub fn set_reply_error(&self, error: String) { self.reply.borrow_mut().replace(Err(error)); }

    /// Execute assertions against the last parsed reply.
    ///
    /// # Panics
    ///
    /// Panics with `"no reply received"` when no reply has been recorded.
    ///
    /// Panics with `"reply parse failed: {error}"` when the stored reply is an
    /// `Err` parse failure.
    pub fn with_reply<T>(&self, f: impl FnOnce(&Transaction) -> T) -> T {
        let reply_ref = self.reply.borrow();
        let Some(reply) = reply_ref.as_ref() else {
            panic!("no reply received");
        };
        let tx = match reply.as_ref() {
            Ok(tx) => tx,
            Err(error) => panic!("reply parse failed: {error}"),
        };
        f(tx)
    }

    /// Authenticate the test connection as the default fixture user.
    ///
    /// The user id argument is ignored; it is retained only to preserve
    /// compatibility with existing BDD step helpers.
    #[expect(
        unused_variables,
        reason = "signature retained for existing BDD step helper compatibility"
    )]
    pub fn authenticate_default_user(&self, user_id: i32) {
        if self.is_skipped() {
            return;
        }
        let outcome = self
            .send_login_with_credentials(b"alice", b"secret")
            .and_then(|reply| {
                if reply.header.error == 0 {
                    return Ok(());
                }
                Err(format!(
                    "login probe failed with error code {}",
                    reply.header.error
                ))
            });
        if let Err(error) = outcome {
            self.set_reply_error(error);
        }
    }

    /// Return true when XOR compatibility appears enabled on this connection.
    ///
    /// The check sends a plaintext login probe and treats authentication failure
    /// as a signal that compatibility decoding is active for text fields.
    #[must_use]
    pub fn is_xor_enabled(&self) -> bool {
        if self.is_skipped() {
            return false;
        }
        let Ok(reply) = self.send_login_with_credentials(b"alice", b"secret") else {
            return false;
        };
        matches!(
            reply.header.error,
            ERR_NOT_AUTHENTICATED | ERR_INTERNAL_SERVER
        )
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::WireframeBddWorld;

    #[test]
    fn set_io_timeout_overrides_default() {
        let world = WireframeBddWorld::new();
        world.set_io_timeout(Duration::from_secs(42));

        assert_eq!(world.io_timeout.get(), Duration::from_secs(42));
    }
}
