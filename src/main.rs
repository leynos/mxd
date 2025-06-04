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

    let pool = establish_pool(&cli.database).await;
    {
        let mut conn = pool.get().await.expect("failed to get db connection");
        run_migrations(&mut conn).await?;
    }

    if let Some(Commands::CreateUser { username, password }) = cli.command {
        let hashed = hash_password(&password);
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
        tokio::spawn(async move {
            if let Err(e) = handle_client(socket, peer, pool).await {
                eprintln!("connection error: {}", e);
            }
        });
    }
}

async fn handle_client(
    socket: TcpStream,
    peer: SocketAddr,
    pool: DbPool,
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

