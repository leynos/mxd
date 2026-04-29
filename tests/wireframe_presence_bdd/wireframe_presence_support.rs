//! Support fixtures for wireframe presence BDD scenarios.

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    sync::{Arc, Mutex},
};

use anyhow::anyhow;
use argon2::Argon2;
use async_trait::async_trait;
use mxd::{
    PresenceRegistry,
    SessionPhase,
    build_notify_delete_user,
    db::{create_user, get_user_by_name},
    field_id::FieldId,
    handler::Session,
    models::NewUser,
    server::outbound::{
        OutboundConnectionId,
        OutboundError,
        OutboundMessaging,
        OutboundPriority,
        OutboundTarget,
    },
    transaction::{Transaction, parse_transaction},
    transaction_type::TransactionType,
    users::hash_password,
    wireframe::{
        compat::XorCompatibility,
        compat_policy::ClientCompatibility,
        connection::HandshakeMetadata,
        router::{RouteContext, WireframeRouter},
        test_helpers::dummy_pool,
    },
};
use test_util::{
    AnyError,
    DatabaseUrl,
    TestDb,
    build_frame,
    build_test_db,
    setup_files_db,
    with_db,
};
use tokio::runtime::Runtime;

pub(super) struct PresenceWorld {
    runtime: Runtime,
    router: WireframeRouter,
    pool: RefCell<mxd::db::DbPool>,
    db_guard: RefCell<Option<TestDb>>,
    presence: PresenceRegistry,
    messaging: RecordingMessaging,
    sessions: RefCell<HashMap<String, Session>>,
    last_transaction: RefCell<Option<Result<Transaction, String>>>,
    skipped: Cell<bool>,
}

#[derive(Clone, Copy)]
pub(super) struct RequestSpec<'a> {
    pub(super) ty: TransactionType,
    pub(super) id: u32,
    pub(super) params: &'a [(FieldId, &'a [u8])],
}

impl PresenceWorld {
    pub(super) fn new() -> Self {
        Self {
            runtime: Runtime::new().unwrap_or_else(|error| panic!("runtime: {error}")),
            router: WireframeRouter::new(
                Arc::new(XorCompatibility::disabled()),
                Arc::new(ClientCompatibility::from_handshake(
                    &HandshakeMetadata::default(),
                )),
            ),
            pool: RefCell::new(dummy_pool()),
            db_guard: RefCell::new(None),
            presence: PresenceRegistry::default(),
            messaging: RecordingMessaging::default(),
            sessions: RefCell::new(HashMap::new()),
            last_transaction: RefCell::new(None),
            skipped: Cell::new(false),
        }
    }

    pub(super) const fn is_skipped(&self) -> bool { self.skipped.get() }

    pub(super) fn setup_db(&self) -> Result<(), AnyError> {
        let Some(test_db) = build_test_db(&self.runtime, setup_presence_db)? else {
            self.skipped.set(true);
            return Ok(());
        };
        self.pool.replace(test_db.pool());
        self.db_guard.replace(Some(test_db));
        Ok(())
    }

    pub(super) fn send(&self, label: &str, request: RequestSpec<'_>) -> Result<(), AnyError> {
        if self.is_skipped() {
            return Ok(());
        }
        let frame = match build_frame(request.ty, request.id, request.params) {
            Ok(frame) => frame,
            Err(error) => {
                self.last_transaction.replace(Some(Err(error.to_string())));
                return Ok(());
            }
        };
        let peer = "127.0.0.1:12345".parse()?;
        let pool = self.pool.borrow().clone();
        let mut sessions = self.sessions.borrow_mut();
        let session = sessions
            .entry(label.to_owned())
            .or_insert_with(session_for_label);
        let reply = self.runtime.block_on(self.router.route(
            &frame,
            RouteContext {
                peer,
                pool,
                session,
                messaging: &self.messaging,
                presence: &self.presence,
                presence_connection_id: connection_id_for_label(label),
            },
        ));
        let parsed = parse_transaction(&reply).map_err(|error| error.to_string());
        self.last_transaction.replace(Some(parsed));
        Ok(())
    }

    pub(super) fn disconnect(&self, label: &str) -> Result<(), AnyError> {
        if self.is_skipped() {
            return Ok(());
        }
        if self.sessions.borrow_mut().remove(label).is_none() {
            return Err(anyhow!("missing session for {label}"));
        }
        let connection_id = connection_id_for_label(label);
        let Some(removal) = self.presence.remove(connection_id) else {
            return Ok(());
        };
        let message = build_notify_delete_user(removal.departed.user_id)?;
        self.runtime.block_on(async move {
            for peer_id in removal.remaining_peer_ids {
                self.messaging
                    .push(
                        OutboundTarget::Connection(peer_id),
                        message.clone(),
                        OutboundPriority::High,
                    )
                    .await
                    .map_err(|error| anyhow!("record notify delete user: {error}"))?;
            }
            Ok::<(), AnyError>(())
        })?;
        Ok(())
    }

    pub(super) fn observe_notification(&self, label: &str) -> Result<Transaction, AnyError> {
        let connection_id = connection_id_for_label(label);
        self.messaging
            .take_next(connection_id)
            .ok_or_else(|| anyhow!("missing queued notification for {label}"))
    }

    pub(super) fn with_last_transaction<T>(
        &self,
        f: impl FnOnce(&Transaction) -> Result<T, AnyError>,
    ) -> Result<T, AnyError> {
        let last_transaction = self.last_transaction.borrow();
        let Some(result) = last_transaction.as_ref() else {
            return Err(anyhow!("no transaction recorded"));
        };
        let Ok(transaction) = result.as_ref() else {
            return Err(anyhow!("transaction failed: {result:?}"));
        };
        f(transaction)
    }
}

#[derive(Clone, Default)]
struct RecordingMessaging {
    inboxes: Arc<Mutex<HashMap<OutboundConnectionId, Vec<Transaction>>>>,
}

impl RecordingMessaging {
    fn take_next(&self, connection_id: OutboundConnectionId) -> Option<Transaction> {
        self.inboxes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get_mut(&connection_id)
            .and_then(|messages| {
                if messages.is_empty() {
                    None
                } else {
                    Some(messages.remove(0))
                }
            })
    }
}

#[async_trait]
impl OutboundMessaging for RecordingMessaging {
    async fn push(
        &self,
        target: OutboundTarget,
        message: Transaction,
        _priority: OutboundPriority,
    ) -> Result<(), OutboundError> {
        let OutboundTarget::Connection(connection_id) = target else {
            return Err(OutboundError::TargetUnavailable);
        };
        self.inboxes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(connection_id)
            .or_default()
            .push(message);
        Ok(())
    }

    async fn broadcast(
        &self,
        _message: Transaction,
        _priority: OutboundPriority,
    ) -> Result<(), OutboundError> {
        Err(OutboundError::MessagingUnavailable)
    }
}

fn connection_id_for_label(label: &str) -> OutboundConnectionId {
    let raw_id = match label {
        "alice-client" => 11,
        "bob-client" => 12,
        other => panic!("unexpected client label {other}"),
    };
    OutboundConnectionId::new(raw_id)
}

fn session_for_label() -> Session {
    Session {
        phase: SessionPhase::Unauthenticated,
        ..Session::default()
    }
}

fn setup_presence_db(db: DatabaseUrl) -> Result<(), AnyError> {
    setup_files_db(db.clone())?;
    with_db(db, |conn| {
        Box::pin(async move {
            if get_user_by_name(conn, "bob").await?.is_none() {
                let hashed = hash_password(&Argon2::default(), "secret")?;
                let new_user = NewUser {
                    username: "bob",
                    password: &hashed,
                };
                create_user(conn, &new_user).await?;
            }
            Ok(())
        })
    })
}
