use std::error::Error;
use std::net::SocketAddr;

use clap::{Parser, Subcommand};
use ortho_config::{OrthoConfig, merge_cli_over_defaults};
use serde::{Deserialize, Serialize};

use tokio::io::{self, AsyncReadExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::task::JoinSet;
use tokio::time::timeout;

use argon2::{Algorithm, Argon2, Params, ParamsBuilder, Version};

use mxd::db::{DbPool, create_user, establish_pool, run_migrations};
use mxd::handler::{Context, Session, handle_request};
use mxd::transaction::{TransactionReader, TransactionWriter};
use mxd::users::hash_password;
use mxd::{models, protocol};

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

fn default_bind() -> String {
    "0.0.0.0:5500".to_string()
}

fn default_db() -> String {
    "mxd.db".to_string()
}

#[derive(OrthoConfig, Serialize, Deserialize, Default, Debug, Clone)]
#[ortho_config(prefix = "MXD_")]
struct AppConfig {
    #[ortho_config(default = default_bind())]
    bind: Option<String>,
    #[ortho_config(default = default_db())]
    database: Option<String>,
    argon2_m_cost: Option<u32>,
    argon2_t_cost: Option<u32>,
    argon2_p_cost: Option<u32>,
}

#[derive(Parser)]
#[command(author, version, about)]
struct CmdLine {
    #[command(flatten)]
    cfg: AppConfigCli,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug, Clone, Deserialize)]
enum Commands {
    /// Create a new user in the database
    CreateUser { username: String, password: String },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = CmdLine::parse();
    let defaults = AppConfig::load_from_iter(["mxd"])?;
    let cli_cfg = AppConfig {
        bind: cli.cfg.bind,
        database: cli.cfg.database,
        argon2_m_cost: cli.cfg.argon2_m_cost,
        argon2_t_cost: cli.cfg.argon2_t_cost,
        argon2_p_cost: cli.cfg.argon2_p_cost,
    };
    let cfg = merge_cli_over_defaults(defaults, cli_cfg);
    let bind = cfg.bind.unwrap_or_else(|| "0.0.0.0:5500".to_string());
    let database = cfg.database.unwrap_or_else(|| "mxd.db".to_string());
    let params = ParamsBuilder::new()
        .m_cost(cfg.argon2_m_cost.unwrap_or(Params::DEFAULT_M_COST))
        .t_cost(cfg.argon2_t_cost.unwrap_or(Params::DEFAULT_T_COST))
        .p_cost(cfg.argon2_p_cost.unwrap_or(Params::DEFAULT_P_COST))
        .build()?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let pool = establish_pool(&database).await;
    {
        let mut conn = pool.get().await.expect("failed to get db connection");
        run_migrations(&mut conn).await?;
    }

    if let Some(Commands::CreateUser { username, password }) = cli.command {
        let hashed = hash_password(&argon2, &password)?;
        let new_user = models::NewUser {
            username: &username,
            password: &hashed,
        };
        let mut conn = pool.get().await.expect("failed to get db connection");
        create_user(&mut conn, &new_user)
            .await
            .expect("failed to create user");
        println!("User {username} created");
        return Ok(());
    }

    let addr = bind;
    let listener = TcpListener::bind(&addr).await?;
    println!("mxd listening on {addr}");

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

async fn handle_client(
    socket: TcpStream,
    peer: SocketAddr,
    pool: DbPool,
    shutdown: &mut watch::Receiver<bool>,
) -> Result<(), Box<dyn Error>> {
    let (mut reader, mut writer) = io::split(socket);

    // perform protocol handshake with a timeout
    let mut buf = [0u8; protocol::HANDSHAKE_LEN];
    match timeout(protocol::HANDSHAKE_TIMEOUT, reader.read_exact(&mut buf)).await {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => return Err(Box::new(e)),
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
    let ctx = Context::new(peer, pool.clone());
    let mut session = Session::default();
    loop {
        tokio::select! {
            tx = tx_reader.read_transaction() => {
                let tx = tx?;
                let frame = tx.to_bytes();
                let resp = handle_request(&ctx, &mut session, &frame).await?;
                tx_writer.write_transaction(&resp).await?;
            }
            _ = shutdown.changed() => {
                break;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use figment::Jail;

    #[test]
    fn env_config_loading() {
        Jail::expect_with(|j| {
            j.set_env("MXD_BIND", "127.0.0.1:8000");
            j.set_env("MXD_DATABASE", "env.db");
            let cfg = AppConfig::load_from_iter(["mxd"]).expect("load");
            assert_eq!(cfg.bind.as_deref(), Some("127.0.0.1:8000"));
            assert_eq!(cfg.database.as_deref(), Some("env.db"));
            Ok(())
        });
    }

    #[test]
    fn cli_overrides_env() {
        Jail::expect_with(|j| {
            j.set_env("MXD_BIND", "127.0.0.1:8000");
            let defaults = AppConfig::load_from_iter(["mxd"]).expect("load");
            let cli = CmdLine::parse_from(["mxd", "--bind", "0.0.0.0:9000"]);
            let cli_cfg = AppConfig {
                bind: cli.cfg.bind,
                database: cli.cfg.database,
                argon2_m_cost: cli.cfg.argon2_m_cost,
                argon2_t_cost: cli.cfg.argon2_t_cost,
                argon2_p_cost: cli.cfg.argon2_p_cost,
            };
            let merged = merge_cli_over_defaults(defaults, cli_cfg);
            assert_eq!(merged.bind.as_deref(), Some("0.0.0.0:9000"));
            Ok(())
        });
    }

    #[test]
    fn loads_from_dotfile() {
        Jail::expect_with(|j| {
            j.create_file(".mxd.toml", "bind = \"1.2.3.4:1111\"")?;
            let cfg = AppConfig::load_from_iter(["mxd"]).expect("load");
            assert_eq!(cfg.bind.as_deref(), Some("1.2.3.4:1111"));
            Ok(())
        });
    }
}
