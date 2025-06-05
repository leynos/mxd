use std::error::Error;
use std::net::SocketAddr;

use clap::{Parser, Subcommand};

use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

mod commands;
mod db;
mod models;
mod schema;
mod users;

use commands::Command;
use db::{DbPool, create_user, establish_pool, run_migrations};
use users::hash_password;

#[derive(Parser)]
#[command(author, version, about)]
struct Cli {
    /// Address to bind the server to
    #[arg(long, default_value = "0.0.0.0:5500")]
    bind: String,

    /// Path to the SQLite database
    #[arg(long, default_value = "mxd.db")]
    database: String,

    /// Argon2 memory cost in KiB
    #[arg(long, default_value_t = Params::DEFAULT_M_COST)]
    argon2_m_cost: u32,

    /// Argon2 iterations
    #[arg(long, default_value_t = Params::DEFAULT_T_COST)]
    argon2_t_cost: u32,

    /// Argon2 parallelism
    #[arg(long, default_value_t = Params::DEFAULT_P_COST)]
    argon2_p_cost: u32,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new user in the database
    CreateUser { username: String, password: String },
}


#[cfg(unix)]
async fn shutdown_signal() {
    let mut sigterm =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {},
        _ = sigterm.recv() => {},
    }
}

#[cfg(not(unix))]
async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install Ctrl-C handler");
}

async fn run_server(listener: TcpListener, pool: DbPool) -> Result<(), Box<dyn Error>> {
    let (shutdown_tx, _) = broadcast::channel::<()>(1);
    let mut tasks: Vec<JoinHandle<()>> = Vec::new();

    loop {
        tokio::select! {
            res = listener.accept() => {
                let (socket, peer) = res?;
                let pool = pool.clone();
                let mut shutdown_rx = shutdown_tx.subscribe();
                tasks.push(tokio::spawn(async move {
                    if let Err(e) = handle_client(socket, peer, pool, &mut shutdown_rx).await {
                        eprintln!("connection error: {}", e);
                    }
                }));
            }
            _ = shutdown_signal() => {
                println!("shutdown signal received");
                break;
            }
        }
    }

    // notify all client tasks to exit
    let _ = shutdown_tx.send(());
    for task in tasks {
        let _ = task.await;
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    let params = ParamsBuilder::new()
        .m_cost(cli.argon2_m_cost)
        .t_cost(cli.argon2_t_cost)
        .p_cost(cli.argon2_p_cost)
        .build()?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let pool = establish_pool(&cli.database).await;
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
        println!("User {} created", username);
        return Ok(());
    }

    let addr = cli.bind;
    let listener = TcpListener::bind(&addr).await?;
    println!("mxd listening on {}", addr);

    run_server(listener, pool).await?;
    Ok(())
}

async fn handle_client(
    socket: TcpStream,
    peer: SocketAddr,
    pool: DbPool,
    shutdown: &mut broadcast::Receiver<()>,
) -> Result<(), Box<dyn Error>> {
    let (reader, mut writer) = io::split(socket);
    let mut lines = BufReader::new(reader).lines();

    writer.write_all(b"MXD\n").await?;
    // process commands until the client closes the connection or shutdown is requested
    loop {
        tokio::select! {
            line = lines.next_line() => {
                match line? {
                    Some(line) => {
                        let line = line.trim();
                        if line.is_empty() {
                            continue;
                        }

                        let mut parts = line.splitn(3, ' ');
                        match parts.next() {
                            Some("LOGIN") => {
                                let (username, password) = match (parts.next(), parts.next()) {
                                    (Some(u), Some(p)) if !u.is_empty() && !p.is_empty() => (u, p),
                                    _ => {
                                        writer.write_all(b"ERR Invalid LOGIN\n").await?;
                                        continue;
                                    }
                                };

                                let hashed = hash_password(password);
                                let mut conn = pool.get().await?;
                                let user = get_user_by_name(&mut conn, username).await?;
                                if user.map(|u| u.password == hashed).unwrap_or(false) {
                                    writer.write_all(b"OK\n").await?;
                                    println!("{} authenticated as {}", peer, username);
                                } else {
                                    writer.write_all(b"FAIL\n").await?;
                                }
                            }
                            _ => {
                                writer.write_all(b"ERR Unknown command\n").await?;
                            }
                        }
                    }
                    None => break,
                }
            }
            _ = shutdown.recv() => {
                break;
            }
        }
    }
    Ok(())
}

