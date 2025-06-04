use std::error::Error;
use std::net::SocketAddr;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

mod db;
mod models;
mod schema;

use db::{establish_pool, get_user_by_name, run_migrations, DbPool};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "mxd.db".into());
    let pool = establish_pool(&database_url);
    {
        let mut conn = pool.get().expect("failed to get db connection");
        run_migrations(&mut conn);
    }

    let addr = std::env::var("MXD_BIND").unwrap_or_else(|_| "0.0.0.0:5500".into());
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

async fn handle_client(mut socket: TcpStream, peer: SocketAddr, pool: DbPool) -> Result<(), Box<dyn Error>> {
    socket.write_all(b"MXD\n").await?;
    let mut buf = vec![0u8; 1024];
    let n = socket.read(&mut buf).await?;
    if n == 0 {
        return Ok(());
    }
    let msg = String::from_utf8_lossy(&buf[..n]);
    let mut parts = msg.trim().splitn(3, ' ');
    match parts.next() {
        Some("LOGIN") => {
            let username = parts.next().unwrap_or("");
            let password = parts.next().unwrap_or("");
            let valid = tokio::task::spawn_blocking(move || {
                let mut conn = pool.get().map_err(|e| Box::new(e) as Box<dyn Error>)?;
                Ok::<_, Box<dyn Error>>(get_user_by_name(&mut conn, username)?.map(|u| u.password == password))
            }).await??;
            if valid.unwrap_or(false) {
                socket.write_all(b"OK\n").await?;
                println!("{} authenticated as {}", peer, username);
            } else {
                socket.write_all(b"FAIL\n").await?;
            }
        }
        _ => {
            socket.write_all(b"ERR Unknown command\n").await?;
        }
    }
    Ok(())
}
