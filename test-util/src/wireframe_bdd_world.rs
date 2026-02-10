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
    field_id::FieldId,
    transaction::{FrameHeader, HEADER_LEN, Transaction},
    transaction_type::TransactionType,
    wireframe::{connection::HandshakeMetadata, test_helpers::build_frame},
};

#[cfg(feature = "postgres")]
use crate::postgres::PostgresTestDbError;
use crate::{AnyError, DatabaseUrl, SetupFn, TestServer, protocol::handshake_with_sub_version};

const IO_TIMEOUT: Duration = Duration::from_secs(10);

/// Shared BDD world backing for binary transport scenarios.
pub struct WireframeBddWorld {
    server: RefCell<Option<TestServer>>,
    stream: RefCell<Option<TcpStream>>,
    reply: RefCell<Option<Result<Transaction, String>>>,
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
            handshake_sub_version: Cell::new(0),
            skipped: Cell::new(false),
        }
    }

    /// Return true when backend availability caused this scenario to be skipped.
    #[must_use]
    pub const fn is_skipped(&self) -> bool { self.skipped.get() }

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

        let mut stream = TcpStream::connect(server_addr)?;
        stream.set_read_timeout(Some(IO_TIMEOUT))?;
        stream.set_write_timeout(Some(IO_TIMEOUT))?;
        handshake_with_sub_version(&mut stream, self.handshake_sub_version.get())?;
        self.stream.borrow_mut().replace(stream);
        Ok(())
    }

    fn read_reply(stream: &mut TcpStream) -> Result<Transaction, AnyError> {
        let mut header_buf = [0u8; HEADER_LEN];
        stream.read_exact(&mut header_buf)?;
        let header = FrameHeader::from_bytes(&header_buf);
        let mut payload = vec![0u8; header.data_size as usize];
        if header.data_size > 0 {
            stream.read_exact(&mut payload)?;
        }
        Ok(Transaction { header, payload })
    }

    fn send_frame(&self, frame: &[u8]) -> Result<Transaction, AnyError> {
        let mut stream_ref = self.stream.borrow_mut();
        let stream = stream_ref
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("wireframe test stream has not been connected"))?;
        stream.write_all(frame)?;
        Self::read_reply(stream)
    }

    fn send_login_with_credentials(&self, username: &[u8], password: &[u8]) -> Result<(), String> {
        let frame = build_frame(
            TransactionType::Login,
            90,
            &[(FieldId::Login, username), (FieldId::Password, password)],
        )
        .map_err(|error| format!("failed to build login frame: {error}"))?;
        let reply = self.send_frame(&frame).map_err(|error| error.to_string())?;
        if reply.header.error == 0 {
            return Ok(());
        }
        Err(format!(
            "login probe failed with error code {}",
            reply.header.error
        ))
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
    pub fn authenticate_default_user(&self, _user_id: i32) {
        if self.is_skipped() {
            return;
        }
        if let Err(error) = self.send_login_with_credentials(b"alice", b"secret") {
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
        self.send_login_with_credentials(b"alice", b"secret")
            .is_err()
    }
}
