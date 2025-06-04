use std::error::Error;
use std::net::SocketAddr;

use clap::{Parser, Subcommand};

use sha2::{Digest, Sha256};
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

mod db;
mod models;
mod schema;

use db::{DbPool, create_user, establish_pool, get_user_by_name, run_migrations};

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

pub(crate) fn hash_password(pw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(pw.as_bytes());
    format!("{:x}", hasher.finalize())
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::hash_password;

    #[test]
    fn test_hash_password() {
        let hashed = hash_password("secret");
        assert_eq!(
            hashed,
            "2bb80d537b1da3e38bd30361aa855686bde0eacd7162fef6a25fe97bf527a25b"
        );
    }
}
