use std::net::SocketAddr;

use crate::db::{CategoryError, DbPool, get_user_by_name, list_article_titles, list_names_at_path};
use crate::field_id::FieldId;
use crate::transaction::{
    FrameHeader, Transaction, decode_params, decode_params_map, encode_params,
};
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
    GetNewsArticleNameList {
        path: String,
        header: FrameHeader,
    },
    GetNewsArticleData {
        path: String,
        article_id: i32,
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
                let params = decode_params_map(&tx.payload).map_err(|_| "invalid params")?;
                let path = params
                    .get(&FieldId::NewsPath)
                    .map(|v| String::from_utf8(v.clone()).map_err(|_| "utf8"))
                    .transpose()?;
                Ok(Command::GetNewsCategoryNameList {
                    path,
                    header: tx.header,
                })
            }
            TransactionType::NewsArticleNameList => {
                let params = decode_params_map(&tx.payload).map_err(|_| "invalid params")?;
                let path = params.get(&FieldId::NewsPath).ok_or("missing path")?;
                Ok(Command::GetNewsArticleNameList {
                    path: String::from_utf8(path.clone()).map_err(|_| "utf8")?,
                    header: tx.header,
                })
            }
            TransactionType::NewsArticleData => {
                let params = decode_params_map(&tx.payload).map_err(|_| "invalid params")?;
                let path = params.get(&FieldId::NewsPath).ok_or("missing path")?;
                let id_bytes = params.get(&FieldId::NewsArticleId).ok_or("missing id")?;
                let id = i32::from_be_bytes(id_bytes.as_slice().try_into().map_err(|_| "id")?);
                Ok(Command::GetNewsArticleData {
                    path: String::from_utf8(path.clone()).map_err(|_| "utf8")?,
                    article_id: id,
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
            } => handle_login(peer, pool, username, password, header).await,
            Command::GetNewsCategoryNameList { header, path } => {
                handle_category_list(pool, header, path).await
            }
            Command::GetNewsArticleNameList { header, path } => {
                handle_article_titles(pool, header, path).await
            }
            Command::GetNewsArticleData {
                header,
                path,
                article_id,
            } => handle_article_data(pool, header, path, article_id).await,
            Command::Unknown { header } => Ok(handle_unknown(peer, header)),
        }
    }
}

fn reply_header(req: &FrameHeader, payload_error: u32, payload_len: usize) -> FrameHeader {
    FrameHeader {
        flags: 0,
        is_reply: 1,
        ty: req.ty,
        id: req.id,
        error: payload_error,
        total_size: payload_len as u32,
        data_size: payload_len as u32,
    }
}

async fn handle_login(
    peer: SocketAddr,
    pool: DbPool,
    username: String,
    password: String,
    header: FrameHeader,
) -> Result<Transaction, Box<dyn std::error::Error>> {
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
        header: reply_header(&header, error, payload.len()),
        payload,
    };
    if error == 0 {
        println!("{} authenticated as {}", peer, username);
    }
    Ok(reply)
}

async fn handle_category_list(
    pool: DbPool,
    header: FrameHeader,
    path: Option<String>,
) -> Result<Transaction, Box<dyn std::error::Error>> {
    let mut conn = pool.get().await?;
    let names = match list_names_at_path(&mut conn, path.as_deref()).await {
        Ok(c) => c,
        Err(CategoryError::InvalidPath) => {
            return Ok(Transaction {
                header: reply_header(&header, NEWS_ERR_PATH_UNSUPPORTED, 0),
                payload: Vec::new(),
            });
        }
        Err(CategoryError::Diesel(e)) => return Err(Box::new(e)),
    };
    let params: Vec<(FieldId, &[u8])> = names
        .iter()
        .map(|c| (FieldId::NewsCategory, c.as_bytes()))
        .collect();
    let payload = encode_params(&params);
    Ok(Transaction {
        header: reply_header(&header, 0, payload.len()),
        payload,
    })
}

async fn handle_article_titles(
    pool: DbPool,
    header: FrameHeader,
    path: String,
) -> Result<Transaction, Box<dyn std::error::Error>> {
    let mut conn = pool.get().await?;
    let names = match list_article_titles(&mut conn, &path).await {
        Ok(c) => c,
        Err(CategoryError::InvalidPath) => {
            return Ok(Transaction {
                header: reply_header(&header, NEWS_ERR_PATH_UNSUPPORTED, 0),
                payload: Vec::new(),
            });
        }
        Err(CategoryError::Diesel(e)) => return Err(Box::new(e)),
    };
    let params: Vec<(FieldId, &[u8])> = names
        .iter()
        .map(|t| (FieldId::NewsArticle, t.as_bytes()))
        .collect();
    let payload = encode_params(&params);
    Ok(Transaction {
        header: reply_header(&header, 0, payload.len()),
        payload,
    })
}

async fn handle_article_data(
    pool: DbPool,
    header: FrameHeader,
    path: String,
    article_id: i32,
) -> Result<Transaction, Box<dyn std::error::Error>> {
    use crate::db::get_article;
    let mut conn = pool.get().await?;
    let article = match get_article(&mut conn, &path, article_id).await {
        Ok(Some(a)) => a,
        Ok(None) | Err(CategoryError::InvalidPath) => {
            return Ok(Transaction {
                header: reply_header(&header, NEWS_ERR_PATH_UNSUPPORTED, 0),
                payload: Vec::new(),
            });
        }
        Err(CategoryError::Diesel(e)) => return Err(Box::new(e)),
    };
    let mut params: Vec<(FieldId, Vec<u8>)> = Vec::new();
    params.push((FieldId::NewsTitle, article.title.into_bytes()));
    if let Some(p) = article.poster {
        params.push((FieldId::NewsPoster, p.into_bytes()));
    }
    params.push((
        FieldId::NewsDate,
        article
            .posted_at
            .and_utc()
            .timestamp()
            .to_be_bytes()
            .to_vec(),
    ));
    if let Some(prev) = article.prev_article_id {
        params.push((FieldId::NewsPrevId, prev.to_be_bytes().to_vec()));
    }
    if let Some(next) = article.next_article_id {
        params.push((FieldId::NewsNextId, next.to_be_bytes().to_vec()));
    }
    if let Some(parent) = article.parent_article_id {
        params.push((FieldId::NewsParentId, parent.to_be_bytes().to_vec()));
    }
    if let Some(child) = article.first_child_article_id {
        params.push((FieldId::NewsFirstChildId, child.to_be_bytes().to_vec()));
    }
    params.push((
        FieldId::NewsArticleFlags,
        (article.flags as i32).to_be_bytes().to_vec(),
    ));
    params.push((FieldId::NewsDataFlavor, b"text/plain".to_vec()));
    if let Some(data) = article.data {
        params.push((FieldId::NewsArticleData, data.into_bytes()));
    }
    let payload_pairs: Vec<(FieldId, &[u8])> =
        params.iter().map(|(id, d)| (*id, d.as_slice())).collect();
    let payload = encode_params(&payload_pairs);
    Ok(Transaction {
        header: reply_header(&header, 0, payload.len()),
        payload,
    })
}

fn handle_unknown(peer: SocketAddr, header: FrameHeader) -> Transaction {
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
    reply
}
