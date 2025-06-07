use std::net::SocketAddr;

use crate::db::{CategoryError, DbPool, get_all_categories, get_user_by_name};
use crate::field_id::FieldId;
use crate::transaction::{FrameHeader, Transaction, decode_params, encode_params};
use crate::transaction_type::TransactionType;
use crate::users::verify_password;

/// Error code used when the requested news path is unsupported.
pub const NEWS_ERR_PATH_UNSUPPORTED: u32 = 1;

pub enum Command {
    Login {
        username: String,
        password: String,
        header: FrameHeader,
    },
    GetNewsCategoryNameList {
        path: Option<String>,
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
                        FieldId::Login => {
                            username = Some(String::from_utf8(data).map_err(|_| "utf8")?)
                        }
                        FieldId::Password => {
                            password = Some(String::from_utf8(data).map_err(|_| "utf8")?)
                        }
                        _ => {}
                    }
                }
                Ok(Command::Login {
                    username: username.ok_or("missing username")?,
                    password: password.ok_or("missing password")?,
                    header: tx.header,
                })
            }
            TransactionType::NewsCategoryNameList => {
                let params = decode_params(&tx.payload).map_err(|_| "invalid params")?;
                let mut path = None;
                for (id, data) in params {
                    if let FieldId::NewsPath = id {
                        path = Some(String::from_utf8(data).map_err(|_| "utf8")?);
                    }
                }
                Ok(Command::GetNewsCategoryNameList {
                    path,
                    header: tx.header,
                })
            }
            _ => Ok(Command::Unknown { header: tx.header }),
        }
    }

    pub async fn process(
        self,
        peer: SocketAddr,
        pool: DbPool,
    ) -> Result<Transaction, Box<dyn std::error::Error>> {
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
                        let params = encode_params(&[(
                            FieldId::Version,
                            &crate::protocol::CLIENT_VERSION.to_be_bytes(),
                        )]);
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
                if error == 0 {
                    println!("{} authenticated as {}", peer, username);
                }
                Ok(reply)
            }
            Command::GetNewsCategoryNameList { header, path } => {
                let mut conn = pool.get().await?;
                let cats = match get_all_categories(&mut conn, path.as_deref()).await {
                    Ok(c) => c,
                    Err(CategoryError::PathFilteringUnimplemented) => {
                        // Non-root paths are currently unsupported, so we
                        // return a stub error reply until filtering is added.
                        return Ok(Transaction {
                            header: reply_header(&header, NEWS_ERR_PATH_UNSUPPORTED, 0),
                            payload: Vec::new(),
                        });
                    }
                    Err(CategoryError::Diesel(e)) => return Err(Box::new(e)),
                };
                let mut payload = Vec::new();
                payload.extend_from_slice(&(cats.len() as u16).to_be_bytes());
                for c in &cats {
                    let fid: u16 = FieldId::NewsCategory.into();
                    payload.extend_from_slice(&fid.to_be_bytes());
                    payload.extend_from_slice(&(c.name.len() as u16).to_be_bytes());
                    payload.extend_from_slice(c.name.as_bytes());
                }
        id: src.id,
        error,
        total_size: payload_len as u32,
        data_size: payload_len as u32,
    }
}
                        error: 0,
                        total_size: payload.len() as u32,
                        data_size: payload.len() as u32,
                    },
                    payload,
                };
                Ok(reply)
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
                println!("{} sent unknown transaction: {}", peer, header.ty);
                Ok(reply)
            }
        }
    }
}
