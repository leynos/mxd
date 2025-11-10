//! Tokio-based legacy Hotline server runtime.
//!
//! These helpers keep the binary thin while making the protocol/session
//! logic available to alternative front-ends (such as the upcoming wireframe
//! adapter) without duplicating code.

use std::{io, net::SocketAddr};

use anyhow::{Context, Result};
use argon2::{Algorithm, Argon2, ParamsBuilder, Version};
use diesel_async::AsyncConnection;
use ortho_config::load_and_merge_subcommand_for;
use tokio::{
    io::{self as tokio_io, AsyncReadExt},
    net::{TcpListener, TcpStream},
    sync::watch,
    task::JoinSet,
    time::timeout,
};
#[cfg(all(feature = "postgres", not(feature = "sqlite")))]
use url::Url;

use super::cli::{AppConfig, Cli, Commands, CreateUserArgs};
use crate::{
    db::{DbConnection, DbPool, apply_migrations, create_user, establish_pool},
    handler::{Context as HandlerContext, Session, handle_request},
    models,
    protocol,
    transaction::{TransactionError, TransactionReader, TransactionWriter},
    users::hash_password,
};

/// Parse CLI arguments and execute the requested action.
///
/// # Errors
///
/// Returns any error encountered while merging configuration or while running
/// the requested command/daemon.
pub async fn dispatch(cli: Cli) -> Result<()> {
    let Cli { config, command } = cli;
    if let Some(command) = command {
        run_command(command, &config).await
    } else {
        run_daemon(config).await
    }
}

/// Execute an administrative command.
///
/// # Errors
///
/// Propagates failures from configuration merging or database operations.
pub async fn run_command(command: Commands, cfg: &AppConfig) -> Result<()> {
    match command {
        Commands::CreateUser(args) => {
            let args = load_and_merge_subcommand_for::<CreateUserArgs>(&args)?;
            run_create_user(args, cfg).await
        }
    }
}

async fn run_create_user(args: CreateUserArgs, cfg: &AppConfig) -> Result<()> {
    let username = args
        .username
        .ok_or_else(|| anyhow::anyhow!("missing username"))?;
    let password = args
        .password
        .ok_or_else(|| anyhow::anyhow!("missing password"))?;

    let argon2 = build_argon2(cfg)?;
    let hashed = hash_password(&argon2, &password)?;
    let new_user = models::NewUser {
        username: &username,
        password: &hashed,
    };
    let mut conn = DbConnection::establish(&cfg.database).await?;
    apply_migrations(&mut conn, &cfg.database).await?;
    create_user(&mut conn, &new_user).await?;
    println!("User {username} created");
    Ok(())
}

/// Run the legacy TCP server using the supplied configuration.
///
/// # Errors
///
/// Returns any failure reported while seeding the database pool, binding the
/// socket, or handling inbound connections.
pub async fn run_daemon(cfg: AppConfig) -> Result<()> {
    let bind = cfg.bind.clone();
    let database = cfg.database.clone();

    // Run Argon2 setup now so the parameters are validated even before the
    // customised instance is threaded into account creation.
    let _ = build_argon2(&cfg)?;

    let pool = setup_database(&database).await?;

    let listener = TcpListener::bind(&bind).await?;
    println!("mxd listening on {bind}");

    accept_connections(listener, pool).await
}

fn build_argon2(cfg: &AppConfig) -> Result<Argon2<'static>> {
    let params = ParamsBuilder::new()
        .m_cost(cfg.argon2_m_cost)
        .t_cost(cfg.argon2_t_cost)
        .p_cost(cfg.argon2_p_cost)
        .build()?;
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

#[cfg(all(feature = "postgres", not(feature = "sqlite")))]
fn is_postgres_url(s: &str) -> bool {
    Url::parse(s)
        .map(|u| matches!(u.scheme(), "postgres" | "postgresql"))
        .unwrap_or(false)
}

async fn create_pool(database: &str) -> DbPool {
    #[cfg(all(feature = "postgres", not(feature = "sqlite")))]
    if is_postgres_url(database) {
        return establish_pool(database).await;
    }
    establish_pool(database).await
}

/// Sets up the database connection pool and runs migrations.
///
/// Establishes a connection pool for the specified database, audits database-specific features,
/// and applies any pending migrations. Returns the initialised connection pool on success.
///
/// # Arguments
///
/// * `database` - The database connection string or file path.
///
/// # Returns
///
/// A result containing the initialised database connection pool, or an error if setup fails.
async fn setup_database(database: &str) -> Result<DbPool> {
    let pool = create_pool(database).await;
    {
        let mut conn = pool.get().await.context("failed to get db connection")?;
        #[cfg(feature = "sqlite")]
        crate::db::audit_sqlite_features(&mut conn).await?;
        #[cfg(all(feature = "postgres", not(feature = "sqlite")))]
        if is_postgres_url(database) {
            crate::db::audit_postgres_features(&mut conn).await?;
        }
        apply_migrations(&mut conn, database).await?;
    }
    Ok(pool)
}

async fn accept_connections(listener: TcpListener, pool: DbPool) -> Result<()> {
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let mut join_set = JoinSet::new();
    let shutdown = shutdown_signal();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            () = &mut shutdown => {
                println!("shutdown signal received");
                break;
            }
            res = listener.accept() => {
                handle_accept_result(res, &pool, &shutdown_rx, &mut join_set);
            }
        }
    }

    // notify all tasks to shut down
    let _ = shutdown_tx.send(true);
    await_spawned_tasks(&mut join_set).await;
    Ok(())
}

fn handle_accept_result(
    res: io::Result<(TcpStream, SocketAddr)>,
    pool: &DbPool,
    shutdown_rx: &watch::Receiver<bool>,
    join_set: &mut JoinSet<()>,
) {
    match res {
        Ok((socket, peer)) => {
            let pool = pool.clone();
            let rx = shutdown_rx.clone();
            spawn_client_handler(socket, peer, pool, rx, join_set);
        }
        Err(e) => eprintln!("accept error: {e}"),
    }
}

fn spawn_client_handler(
    socket: TcpStream,
    peer: SocketAddr,
    pool: DbPool,
    mut shutdown_rx: watch::Receiver<bool>,
    join_set: &mut JoinSet<()>,
) {
    join_set.spawn(async move {
        if let Err(e) = handle_client(socket, peer, pool, &mut shutdown_rx).await {
            eprintln!("connection error from {peer}: {e}");
        }
    });
}

async fn await_spawned_tasks(join_set: &mut JoinSet<()>) {
    while let Some(res) = join_set.join_next().await {
        if let Err(e) = res {
            eprintln!("task error: {e}");
        }
    }
}

/// Handles a single client connection, performing handshake and processing transactions.
async fn handle_client(
    socket: TcpStream,
    peer: SocketAddr,
    pool: DbPool,
    shutdown: &mut watch::Receiver<bool>,
) -> Result<()> {
    let (mut reader, mut writer) = tokio_io::split(socket);

    // perform protocol handshake with a timeout
    let mut buf = [0u8; protocol::HANDSHAKE_LEN];
    match timeout(protocol::HANDSHAKE_TIMEOUT, reader.read_exact(&mut buf)).await {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => {
            if e.kind() == io::ErrorKind::UnexpectedEof {
                // Client disconnected before completing the handshake
                return Ok(());
            }
            return Err(e.into());
        }
        Err(_) => {
            protocol::write_handshake_reply(&mut writer, protocol::HANDSHAKE_ERR_TIMEOUT).await?;
            return Ok(());
        }
    }
    match protocol::parse_handshake(&buf) {
        Ok(_) => {
            protocol::write_handshake_reply(&mut writer, protocol::HANDSHAKE_OK).await?;
        }
        Err(err) => {
            let code = protocol::handshake_error_code(&err);
            protocol::write_handshake_reply(&mut writer, code).await?;
            return Ok(());
        }
    }

    let mut tx_reader = TransactionReader::new(reader);
    let mut tx_writer = TransactionWriter::new(writer);
    let ctx = HandlerContext::new(peer, pool.clone());
    let mut session = Session::default();
    loop {
        tokio::select! {
            tx = tx_reader.read_transaction() => match tx {
                Ok(tx) => {
                    let frame = tx.to_bytes();
                    let resp = handle_request(&ctx, &mut session, &frame)
                        .await
                        .map_err(|e| anyhow::anyhow!(e))?;
                    tx_writer.write_transaction(&resp).await?;
                }
                Err(TransactionError::Io(ref e)) if e.kind() == io::ErrorKind::UnexpectedEof => {
                    // Remote closed the connection, end session gracefully
                    break;
                }
                Err(e) => return Err(e.into()),
            },
            _ = shutdown.changed() => {
                break;
            }
        }
    }
    Ok(())
}

/// Waits for a shutdown signal, completing when termination is requested.
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut term) => {
                tokio::select! {
                    res = tokio::signal::ctrl_c() => {
                        if let Err(err) = res {
                            eprintln!("failed to listen for Ctrl-C: {err}");
                        }
                    },
                    _ = term.recv() => {},
                }
            }
            Err(err) => {
                eprintln!("failed to install SIGTERM handler: {err}");
                wait_for_ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    {
        wait_for_ctrl_c().await;
    }
}

async fn wait_for_ctrl_c() {
    if let Err(err) = tokio::signal::ctrl_c().await {
        eprintln!("failed to listen for Ctrl-C: {err}");
    }
}

#[cfg(test)]
mod tests {
    #[cfg(all(feature = "postgres", not(feature = "sqlite")))]
    #[test]
    fn postgres_url_detection() {
        assert!(super::is_postgres_url("postgres://localhost"));
        assert!(super::is_postgres_url("postgresql://localhost"));
        assert!(!super::is_postgres_url("sqlite://localhost"));
    }
}
