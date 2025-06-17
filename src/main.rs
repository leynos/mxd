//! Command line interface for the server.
//!
//! Provides subcommands to start the daemon, manage the database and
//! create users. This is the primary entry point used when running `mxd`.
#![allow(non_snake_case)]
use std::{io, net::SocketAddr};

use anyhow::Result;
use argon2::{Algorithm, Argon2, Params, ParamsBuilder, Version};
use clap::{Args, Parser, Subcommand};
use clap_dispatch::clap_dispatch;
use diesel_async::AsyncConnection;
#[cfg(feature = "postgres")]
use mxd::db::audit_postgres_features;
#[cfg(feature = "sqlite")]
use mxd::db::audit_sqlite_features;
use mxd::{
    db::{DbConnection, DbPool, apply_migrations, create_user, establish_pool},
    handler::{Context as HandlerContext, Session, handle_request},
    models,
    protocol,
    transaction::{TransactionError, TransactionReader, TransactionWriter},
    users::hash_password,
};
use ortho_config::{OrthoConfig, load_and_merge_subcommand_for};
use serde::{Deserialize, Serialize};
use tokio::{
    io::{self as tokio_io, AsyncReadExt},
    net::{TcpListener, TcpStream},
    sync::watch,
    task::JoinSet,
    time::timeout,
};
#[cfg(feature = "postgres")]
use url::Url;

/// Waits for a shutdown signal, completing when termination is requested.
///
/// On Unix platforms, listens for either SIGTERM or Ctrl-C. On non-Unix platforms, listens for
/// Ctrl-C only. The function returns when any of these signals are received, allowing for graceful
/// shutdown of the application.
///
/// # Examples
///
/// ```
/// tokio::spawn(async {
///     shutdown_signal().await;
///     println!("Shutdown signal received.");
/// });
/// ```
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut term = signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = term.recv() => {},
        }
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl-C handler");
    }
}

#[derive(Parser, Deserialize, Serialize, Default, Debug, Clone, OrthoConfig)]
#[ortho_config(prefix = "MXD_")]
struct CreateUserArgs {
    username: Option<String>,
    password: Option<String>,
}

#[clap_dispatch(fn run(self, cfg: &AppConfig) -> Result<()>)]
#[derive(Subcommand, Deserialize, Serialize, Debug, Clone)]
enum Commands {
    #[command(name = "create-user")]
    CreateUser(CreateUserArgs),
}

impl Run for CreateUserArgs {
    /// Creates a new user with the specified username and password, hashing the password securely
    /// and storing the user in the database.
    ///
    /// Validates that both username and password are provided, hashes the password using Argon2id
    /// with parameters from the configuration, runs database migrations if necessary, and inserts
    /// the new user record. Prints a confirmation message upon successful creation.
    ///
    /// # Errors
    ///
    /// Returns an error if required arguments are missing, password hashing fails, database
    /// connection or migrations fail, or user creation is unsuccessful.
    fn run(self, cfg: &AppConfig) -> Result<()> {
        tokio::runtime::Handle::current().block_on(async {
            let username = self
                .username
                .ok_or_else(|| anyhow::anyhow!("missing username"))?;
            let password = self
                .password
                .ok_or_else(|| anyhow::anyhow!("missing password"))?;

            let params = ParamsBuilder::new()
                .m_cost(cfg.argon2_m_cost)
                .t_cost(cfg.argon2_t_cost)
                .p_cost(cfg.argon2_p_cost)
                .build()?;
            let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
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
        })
    }
}

#[allow(non_snake_case)]
#[derive(Args, OrthoConfig, Serialize, Deserialize, Default, Debug, Clone)]
#[ortho_config(prefix = "MXD_")]
struct AppConfig {
    #[ortho_config(default = "0.0.0.0:5500".to_string())]
    #[arg(long, default_value_t = String::from("0.0.0.0:5500"))]
    bind: String,
    #[ortho_config(default = "mxd.db".to_string())]
    #[arg(long, default_value_t = String::from("mxd.db"))]
    database: String,
    #[ortho_config(default = Params::DEFAULT_M_COST)]
    #[arg(long, default_value_t = Params::DEFAULT_M_COST)]
    argon2_m_cost: u32,
    #[ortho_config(default = Params::DEFAULT_T_COST)]
    #[arg(long, default_value_t = Params::DEFAULT_T_COST)]
    argon2_t_cost: u32,
    #[ortho_config(default = Params::DEFAULT_P_COST)]
    #[arg(long, default_value_t = Params::DEFAULT_P_COST)]
    argon2_p_cost: u32,
}

#[derive(Parser)]
struct Cli {
    #[command(flatten)]
    config: AppConfig,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let cfg = cli.config;
    if let Some(cmd) = cli.command {
        let cmd = match cmd {
            Commands::CreateUser(args) => {
                Commands::CreateUser(load_and_merge_subcommand_for::<CreateUserArgs>(&args)?)
            }
        };
        return cmd.run(&cfg);
    }

    let bind = cfg.bind;
    let database = cfg.database;

    let params = ParamsBuilder::new()
        .m_cost(cfg.argon2_m_cost)
        .t_cost(cfg.argon2_t_cost)
        .p_cost(cfg.argon2_p_cost)
        .build()?;
    // Placeholder: use customized Argon2 instance when creating accounts
    let _argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let pool = setup_database(&database).await?;

    let listener = TcpListener::bind(&bind).await?;
    println!("mxd listening on {bind}");

    accept_connections(listener, pool).await
}

#[cfg(feature = "postgres")]
fn is_postgres_url(s: &str) -> bool {
    Url::parse(s)
        .map(|u| matches!(u.scheme(), "postgres" | "postgresql"))
        .unwrap_or(false)
}

async fn create_pool(database: &str) -> DbPool {
    #[cfg(feature = "postgres")]
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
///
/// # Examples
///
/// ```
/// let pool = setup_database("mxd.db").await?; 
/// ```
async fn setup_database(database: &str) -> Result<DbPool> {
    let pool = create_pool(database).await;
    {
        let mut conn = pool.get().await.expect("failed to get db connection");
        #[cfg(feature = "sqlite")]
        audit_sqlite_features(&mut conn).await?;
        #[cfg(feature = "postgres")]
        audit_postgres_features(&mut conn).await?;
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
                match res {
                    Ok((socket, peer)) => {
                        let pool = pool.clone();
                        let mut rx = shutdown_rx.clone();
                        join_set.spawn(async move {
                            if let Err(e) = handle_client(socket, peer, pool, &mut rx).await {
                                eprintln!("connection error from {peer}: {e}");
                            }
                        });
                    }
                    Err(e) => {
                        eprintln!("accept error: {e}");
                    }
                }
            }
        }
    }

    // notify all tasks to shut down
    let _ = shutdown_tx.send(true);

    while let Some(res) = join_set.join_next().await {
        if let Err(e) = res {
            eprintln!("task error: {e}");
        }
    }

    Ok(())
}

/// Handles a single client connection, performing handshake and processing transactions.
///
/// Performs a protocol handshake with the client, responding to handshake errors or timeouts as
/// appropriate. After a successful handshake, enters a loop to read and process transactions from
/// the client, sending responses back. Gracefully handles client disconnects and server shutdown
/// signals.
///
/// # Arguments
///
/// - `socket`: The TCP stream representing the client connection.
/// - `peer`: The client's socket address.
/// - `pool`: The database connection pool.
/// - `shutdown`: A watch channel receiver used to signal server shutdown.
///
/// # Returns
///
/// Returns `Ok(())` on normal termination, or an error if a protocol or I/O error occurs outside of
/// expected disconnects.
///
/// # Examples
///
/// ```no_run
/// # use tokio::net::TcpStream;
/// # use std::net::SocketAddr;
/// # use mxd::db::DbPool;
/// # use tokio::sync::watch;
/// # async fn example(socket: TcpStream, peer: SocketAddr, pool: DbPool, mut shutdown: watch::Receiver<bool>) {
/// let _ = handle_client(socket, peer, pool, &mut shutdown).await;
/// # }
/// ```
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

#[cfg(test)]
mod tests {
    use figment::Jail;

    use super::*;

    #[test]
    fn env_config_loading() {
        Jail::expect_with(|j| {
            j.set_env("MXD_BIND", "127.0.0.1:8000");
            j.set_env("MXD_DATABASE", "env.db");
            let cfg = AppConfig::load_from_iter(["mxd"]).expect("load");
            assert_eq!(cfg.bind, "127.0.0.1:8000");
            assert_eq!(cfg.database, "env.db".to_string());
            Ok(())
        });
    }

    #[test]
    fn cli_overrides_env() {
        Jail::expect_with(|j| {
            j.set_env("MXD_BIND", "127.0.0.1:8000");
            let cfg = AppConfig::load_from_iter(["mxd", "--bind", "0.0.0.0:9000"]).expect("load");
            assert_eq!(cfg.bind, "0.0.0.0:9000");
            Ok(())
        });
    }

    #[test]
    fn loads_from_dotfile() {
        Jail::expect_with(|j| {
            j.create_file(".mxd.toml", "bind = \"1.2.3.4:1111\"")?;
            let cfg = AppConfig::load_from_iter(["mxd"]).expect("load");
            assert_eq!(cfg.bind, "1.2.3.4:1111".to_string());
            Ok(())
        });
    }

    #[test]
    fn argon2_cli() {
        Jail::expect_with(|_j| {
            let cfg = AppConfig::load_from_iter(["mxd", "--argon2-m-cost", "1024"]).expect("load");
            assert_eq!(cfg.argon2_m_cost, 1024);
            Ok(())
        });
    }
}
