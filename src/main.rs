use std::error::Error;
use std::net::SocketAddr;

use clap::{Parser, Subcommand};

use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

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

    loop {
        let (socket, peer) = listener.accept().await?;
        let pool = pool.clone();
        let argon2 = argon2.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_client(socket, peer, pool, argon2).await {
                eprintln!("connection error: {}", e);
            }
        });
    }
}

async fn handle_client(
    socket: TcpStream,
    peer: SocketAddr,
    pool: DbPool,
    argon2: Argon2<'static>,
) -> Result<(), Box<dyn Error>> {
    let (reader, mut writer) = io::split(socket);
    let mut lines = BufReader::new(reader).lines();

    writer.write_all(b"MXD\n").await?;

    // process commands until the client closes the connection
    while let Some(line) = lines.next_line().await? {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match Command::parse(line) {
            Ok(cmd) => {
                cmd.dispatch(peer, &mut writer, pool.clone()).await?;
            }
            Err(err) => {
                writer
                    .write_all(format!("ERR {}\n", err).as_bytes())
                    .await?;
            }
        }
    }
    Ok(())
}

