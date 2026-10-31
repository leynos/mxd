//! Unit tests for legacy server helpers, ensuring internal behaviours remain
//! stable without requiring the external binary.

use std::sync::Arc;

use anyhow::Result;
use argon2::Argon2;
#[cfg(feature = "sqlite")]
use diesel_async::{AsyncConnection, RunQueryDsl};
#[cfg(all(feature = "postgres", not(feature = "sqlite")))]
use rstest::rstest;
#[cfg(feature = "sqlite")]
use tempfile::TempDir;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::watch,
    task::JoinSet,
};

use super::{ServerResources, handle_accept_result, test_helpers};
use crate::{presence::PresenceRegistry, protocol};

#[cfg(all(feature = "postgres", not(feature = "sqlite")))]
#[rstest]
#[case("postgres://localhost", true)]
#[case("postgresql://localhost", true)]
#[case("sqlite://localhost", false)]
fn postgres_url_detection(#[case] url: &str, #[case] expected: bool) {
    assert_eq!(super::is_postgres_url(url), expected);
}

#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
#[tokio::test]
async fn handle_accept_result_shares_argon2_between_clients() -> Result<()> {
    let pool = test_helpers::dummy_pool();
    let argon2 = Arc::new(Argon2::default());
    let strong_before = Arc::strong_count(&argon2);
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let mut join_set = JoinSet::new();
    let resources = ServerResources {
        pool,
        argon2: Arc::clone(&argon2),
        presence: Arc::new(PresenceRegistry::default()),
    };
    // resources holds one clone; count is now strong_before + 1
    let after_resources = Arc::strong_count(&argon2);

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let mut client = TcpStream::connect(addr).await?;
    let (server_socket, peer) = listener.accept().await?;

    handle_accept_result(
        Ok((server_socket, peer)),
        &resources,
        &shutdown_rx,
        &mut join_set,
    );

    // Spawned task receives a clone of resources, adding one more reference.
    assert_eq!(Arc::strong_count(&argon2), after_resources + 1);

    client.write_all(&test_helpers::handshake_frame()).await?;
    let mut reply = [0u8; protocol::REPLY_LEN];
    client.read_exact(&mut reply).await?;
    client.shutdown().await?;

    shutdown_tx
        .send(true)
        .expect("shutdown receivers should remain until broadcast");

    while let Some(result) = join_set.join_next().await {
        result.expect("client handler task");
    }

    // After task completes, only the original and resources clone remain.
    assert_eq!(Arc::strong_count(&argon2), strong_before + 1);

    Ok(())
}

/// Count the rows in a table, or return `None` when it does not exist.
///
/// `sqlite_master` is consulted first so that a missing table reads as "nothing
/// was written" rather than as a query error; `COUNT(*)` always yields a row, so
/// `BigInt` is the correct `sql_type` for the aggregate.
#[cfg(feature = "sqlite")]
async fn count_rows(conn: &mut crate::db::DbConnection, table: &str) -> Option<i64> {
    use diesel::{
        QueryableByName,
        sql_query,
        sql_types::{BigInt, Text},
    };

    #[derive(QueryableByName)]
    struct RowCount {
        #[diesel(sql_type = BigInt)]
        total: i64,
    }

    #[derive(QueryableByName)]
    struct TableExists {
        #[diesel(sql_type = BigInt)]
        present: i64,
    }

    let exists = sql_query(
        "SELECT COUNT(*) AS present FROM sqlite_master WHERE type = 'table' AND name = ?",
    )
    .bind::<Text, _>(table)
    .get_result::<TableExists>(conn)
    .await
    .expect("sqlite_master must be queryable");
    if exists.present == 0 {
        return None;
    }

    let query = format!("SELECT COUNT(*) AS total FROM \"{table}\"");
    let count = sql_query(query)
        .get_result::<RowCount>(conn)
        .await
        .expect("row count must be readable once the table exists");
    Some(count.total)
}

/// A legacy start-up audit failure must stop the migration runner.
///
/// `setup_database` audits before it calls `apply_migrations`, and that ordering
/// is what makes the check worth having: a library that cannot execute the
/// probes must fail the server rather than migrate a schema the server cannot
/// then use. Nothing else in the suite reaches this ordering, because every
/// other test either uses the in-memory pool or never gets past argument
/// parsing.
///
/// The assertion is on table absence rather than a zero row count, so it cannot
/// pass vacuously on a migration table that was created and left empty.
#[cfg(feature = "sqlite")]
#[tokio::test]
async fn setup_database_stops_before_migrating_when_the_audit_fails() {
    let temp_dir = TempDir::new().expect("temporary directory must be created");
    let database = temp_dir.path().join("audited.mxd.db");

    let error = {
        let _injection = crate::db::fail_next_audit();
        super::setup_database(&database.to_string_lossy(), None)
            .await
            .expect_err("an audit failure must abort start-up")
    };

    assert!(
        format!("{error:#}").contains("SQLite capability audit failed"),
        "the start-up error must carry the audit context: {error:#}"
    );

    let mut conn = crate::db::DbConnection::establish(&database.to_string_lossy())
        .await
        .expect("the database file must be openable");
    assert_eq!(
        count_rows(&mut conn, "__diesel_schema_migrations").await,
        None,
        "no migration may run when the audit fails"
    );
}
