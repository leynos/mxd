use std::net::SocketAddr;

use tokio::io::AsyncWriteExt;

use crate::db::{DbPool, get_user_by_name};

use crate::users::verify_password;

pub enum Command {
    Login { username: String, password: String },
    Unknown(String),
}



impl Command {
    pub fn parse(line: &str) -> Result<Self, &'static str> {
        let mut parts = line.splitn(3, ' ');
        match parts.next() {
            Some("LOGIN") => {
                let (username, password) = match (parts.next(), parts.next()) {
                    (Some(u), Some(p)) if !u.is_empty() && !p.is_empty() => (u, p),
                    _ => return Err("Invalid LOGIN"),
                };
                Ok(Command::Login {
                    username: username.to_string(),
                    password: password.to_string(),
                })
            }
            Some(cmd) => Ok(Command::Unknown(cmd.to_string())),
            None => Err("Empty command"),
        }
    }

    pub async fn dispatch<W>(
        self,
        peer: SocketAddr,
        writer: &mut W,
        pool: DbPool,
    ) -> Result<(), Box<dyn std::error::Error>>
    where
        W: AsyncWriteExt + Unpin,
    {
        match self {
            Command::Login { username, password } => {
                let mut conn = pool.get().await?;
                let user = get_user_by_name(&mut conn, &username).await?;
                if let Some(u) = user {
                    if verify_password(&u.password, &password) {
                        writer.write_all(b"OK\n").await?;
                        println!("{} authenticated as {}", peer, username);
                    } else {
                        writer.write_all(b"FAIL\n").await?;
                    }
                } else {
                    writer.write_all(b"FAIL\n").await?;
                }
            }
            Command::Unknown(cmd) => {
                writer.write_all(b"ERR Unknown command\n").await?;
                println!("{} sent unknown command: {}", peer, cmd);
            }
        }
        Ok(())
    }
}
