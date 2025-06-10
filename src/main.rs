use anyhow::{Context, Result};
use std::net::SocketAddr;

use argon2::{Algorithm, Argon2, Params, ParamsBuilder, Version};
use ortho_config::OrthoConfig;
use serde::{Deserialize, Serialize};

use tokio::io::{self, AsyncReadExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::task::JoinSet;
use tokio::time::timeout;

use mxd::db::{DbPool, establish_pool, run_migrations};
use mxd::handler::{Context as HandlerContext, Session, handle_request};
use mxd::protocol;
use mxd::transaction::{TransactionReader, TransactionWriter};

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

#[derive(OrthoConfig, Serialize, Deserialize, Default, Debug, Clone)]
#[ortho_config(prefix = "MXD_")]
struct AppConfig {
    #[ortho_config(default = "0.0.0.0:5500".to_string())]
    bind: String,
    #[ortho_config(default = "mxd.db".to_string())]
    database: String,
    #[ortho_config(default = Params::DEFAULT_M_COST)]
    argon2_m_cost: u32,
    #[ortho_config(default = Params::DEFAULT_T_COST)]
    argon2_t_cost: u32,
    #[ortho_config(default = Params::DEFAULT_P_COST)]
    argon2_p_cost: u32,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = AppConfig::load().context("loading configuration")?;
    let bind = cfg.bind;
    let database = cfg.database;

    let params = ParamsBuilder::new()
        .m_cost(cfg.argon2_m_cost)
        .t_cost(cfg.argon2_t_cost)
        .p_cost(cfg.argon2_p_cost)
        .build()?;
    // Placeholder: use customized Argon2 instance when creating accounts
    let _argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let pool = establish_pool(&database).await;
    {
        let mut conn = pool.get().await.expect("failed to get db connection");
        run_migrations(&mut conn).await?;
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
) -> Result<()> {
    let (mut reader, mut writer) = io::split(socket);

    // perform protocol handshake with a timeout
    let mut buf = [0u8; protocol::HANDSHAKE_LEN];
    match timeout(protocol::HANDSHAKE_TIMEOUT, reader.read_exact(&mut buf)).await {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => return Err(e.into()),
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
            tx = tx_reader.read_transaction() => {
                let tx = tx?;
                let frame = tx.to_bytes();
                let resp = handle_request(&ctx, &mut session, &frame)
                    .await
                    .map_err(|e| anyhow::anyhow!(e))?;
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
    use figment::Jail;

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
        Jail::expect_with(|j| {
            let cfg = AppConfig::load_from_iter(["mxd", "--argon2-m-cost", "1024"]).expect("load");
            assert_eq!(cfg.argon2_m_cost, 1024);
            Ok(())
        });
    }
}
