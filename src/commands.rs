use std::net::SocketAddr;

use tokio::io::AsyncWriteExt;

use crate::db::{DbPool, get_user_by_name};
use crate::transaction::{
    FrameHeader, Transaction, TransactionWriter, decode_params, encode_params,
};
use crate::transaction_type::TransactionType;
use crate::users::verify_password;

pub enum Command {
    Login {
        username: String,
        password: String,
        header: FrameHeader,
    },
    Unknown {
        header: FrameHeader,
    },
}

impl Command {
    pub fn from_transaction(tx: Transaction) -> Result<Self, &'static str> {
        match TransactionType::from(tx.header.ty) {
            TransactionType::Login => {
                let params = decode_params(&tx.payload).map_err(|_| "invalid params")?;
                let mut username = None;
                let mut password = None;
                for (id, data) in params {
                    match id {
                        105 => username = Some(String::from_utf8(data).map_err(|_| "utf8")?),
                        106 => password = Some(String::from_utf8(data).map_err(|_| "utf8")?),
                        _ => {}
                    }
                }
                Ok(Command::Login {
                    username: username.ok_or("missing username")?,
                    password: password.ok_or("missing password")?,
                    header: tx.header,
                })
            }
            _ => Ok(Command::Unknown { header: tx.header }),
        }
    }

    pub async fn dispatch<W>(
        self,
        peer: SocketAddr,
        writer: &mut TransactionWriter<W>,
        pool: DbPool,
    ) -> Result<(), Box<dyn std::error::Error>>
    where
        W: AsyncWriteExt + Unpin,
    {
        match self {
            Command::Login {
                username,
                password,
                header,
            } => {
                let mut conn = pool.get().await?;
                let user = get_user_by_name(&mut conn, &username).await?;
                let (error, payload) = if let Some(u) = user {
                    if verify_password(&u.password, &password) {
                        let params =
                            encode_params(&[(160, &crate::protocol::CLIENT_VERSION.to_be_bytes())]);
                        (0u32, params)
                    } else {
                        (1u32, Vec::new())
                    }
                } else {
                    (1u32, Vec::new())
                };
                let reply = Transaction {
                    header: FrameHeader {
                        flags: 0,
                        is_reply: 1,
                        ty: header.ty,
                        id: header.id,
                        error,
                        total_size: payload.len() as u32,
                        data_size: payload.len() as u32,
                    },
                    payload,
                };
                writer.write_transaction(&reply).await?;
                if error == 0 {
                    println!("{} authenticated as {}", peer, username);
                }
            }
            Command::Unknown { header } => {
                let reply = Transaction {
                    header: FrameHeader {
                        flags: 0,
                        is_reply: 1,
                        ty: header.ty,
                        id: header.id,
                        error: 1,
                        total_size: 0,
                        data_size: 0,
                    },
                    payload: Vec::new(),
                };
                writer.write_transaction(&reply).await?;
                println!("{} sent unknown transaction: {}", peer, header.ty);
            }
        }
        Ok(())
    }
}
