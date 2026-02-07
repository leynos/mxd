//! Shared world state and helpers for wireframe BDD routing tests.

use std::{
    cell::{Cell, RefCell},
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
};

use anyhow::Context as _;
use mxd::{
    db::DbPool,
    handler::Session,
    privileges::Privileges,
    server::outbound::NoopOutboundMessaging,
    transaction::{Transaction, parse_transaction},
    wireframe::{
        compat::XorCompatibility,
        compat_policy::ClientCompatibility,
        connection::HandshakeMetadata,
        routes::{RouteContext, process_transaction_bytes},
        test_helpers::dummy_pool,
    },
};
use tokio::runtime::Runtime;

use crate::{AnyError, SetupFn, TestDb, bdd_helpers::build_test_db};

/// Shared BDD world backing for wireframe routing-focused scenarios.
pub struct WireframeBddWorld {
    runtime: Runtime,
    peer: SocketAddr,
    pool: RefCell<DbPool>,
    db_guard: RefCell<Option<TestDb>>,
    session: RefCell<Session>,
    reply: RefCell<Option<Result<Transaction, String>>>,
    compat: Arc<XorCompatibility>,
    client_compat: RefCell<Arc<ClientCompatibility>>,
    skipped: Cell<bool>,
}

impl WireframeBddWorld {
    /// Create a fresh wireframe BDD world with default compatibility settings.
    ///
    /// # Errors
    ///
    /// Returns an error when the Tokio runtime for this scenario world cannot
    /// be created.
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let peer = SocketAddr::from((Ipv4Addr::LOCALHOST, 12_345));
        let runtime = Runtime::new()?;
        Ok(Self {
            runtime,
            peer,
            pool: RefCell::new(dummy_pool()),
            db_guard: RefCell::new(None),
            session: RefCell::new(Session::default()),
            reply: RefCell::new(None),
            compat: Arc::new(XorCompatibility::disabled()),
            client_compat: RefCell::new(Arc::new(ClientCompatibility::from_handshake(
                &HandshakeMetadata::default(),
            ))),
            skipped: Cell::new(false),
        })
    }

    /// Return true when backend availability caused this scenario to be skipped.
    #[must_use]
    pub const fn is_skipped(&self) -> bool { self.skipped.get() }

    /// Build and install a fixture database for this scenario.
    ///
    /// # Errors
    ///
    /// Returns an error when fixture database construction fails.
    pub fn setup_db(&self, setup: SetupFn) -> Result<(), AnyError> {
        if self.is_skipped() {
            return Ok(());
        }
        let db = match build_test_db(&self.runtime, setup) {
            Ok(Some(db)) => db,
            Ok(None) => {
                self.skipped.set(true);
                return Ok(());
            }
            Err(err) => return Err(err).context("failed to set up database"),
        };
        self.pool.replace(db.pool());
        self.db_guard.replace(Some(db));
        self.session.replace(Session::default());
        Ok(())
    }

    /// Update client compatibility policy from handshake metadata.
    pub fn set_client_compat_from_handshake(&self, handshake: &HandshakeMetadata) {
        let compat = Arc::new(ClientCompatibility::from_handshake(handshake));
        self.client_compat.replace(compat);
    }

    /// Route a raw frame through wireframe transaction processing.
    pub fn send_raw(&self, frame: &[u8]) {
        if self.is_skipped() {
            return;
        }
        let pool = self.pool.borrow().clone();
        let peer = self.peer;
        // In `send_raw`, we intentionally use `session.replace(Session::default())`
        // to swap out state before `runtime.block_on(process_transaction_bytes(...))`
        // and restore it afterwards. If `process_transaction_bytes` panics, the
        // original session is dropped; that fragility is acceptable in test code,
        // but production paths should avoid this or use a safer take/restore pattern.
        let mut session = self.session.replace(Session::default());
        let messaging = NoopOutboundMessaging;
        let compat = Arc::clone(&self.compat);
        let client_compat = Arc::clone(&self.client_compat.borrow());
        let reply = self.runtime.block_on(process_transaction_bytes(
            frame,
            RouteContext {
                peer,
                pool,
                session: &mut session,
                messaging: &messaging,
                compat: compat.as_ref(),
                client_compat: client_compat.as_ref(),
            },
        ));
        self.session.replace(session);
        let outcome = parse_transaction(&reply).map_err(|err| err.to_string());
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

    /// Mark the session as authenticated with default user privileges.
    pub fn authenticate_default_user(&self, user_id: i32) {
        if self.is_skipped() {
            return;
        }
        let mut session = self.session.borrow_mut();
        session.user_id = Some(user_id);
        session.privileges = Privileges::default_user();
    }

    /// Return true when XOR compatibility has been enabled by observed traffic.
    #[must_use]
    pub fn is_xor_enabled(&self) -> bool { self.compat.is_enabled() }
}
