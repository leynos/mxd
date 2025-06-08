use std::io::{Read, Write};
use std::net::TcpStream;

use diesel_async::AsyncConnection;
use mxd::commands::NEWS_ERR_PATH_UNSUPPORTED;
use mxd::db::{DbConnection, create_category, run_migrations};
use mxd::field_id::FieldId;
use mxd::models::NewCategory;
use mxd::transaction::{FrameHeader, Transaction, encode_params};
use mxd::transaction_type::TransactionType;
use test_util::TestServer;

#[test]
fn list_news_articles_invalid_path() -> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start_with_setup("./Cargo.toml", |db| {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async {
            let mut conn = DbConnection::establish(db.to_str().unwrap()).await?;
            run_migrations(&mut conn).await?;
            create_category(
                &mut conn,
                &NewCategory {
                    name: "General",
                    bundle_id: None,
                },
            )
            .await?;
            Ok(())
        })
    })?;

    let port = server.port();
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    let mut handshake = Vec::new();
    handshake.extend_from_slice(b"TRTP");
    handshake.extend_from_slice(&0u32.to_be_bytes());
    handshake.extend_from_slice(&1u16.to_be_bytes());
    handshake.extend_from_slice(&0u16.to_be_bytes());
    stream.write_all(&handshake)?;

    let mut reply = [0u8; 8];
    stream.read_exact(&mut reply)?;
    assert_eq!(&reply[0..4], b"TRTP");
    assert_eq!(u32::from_be_bytes(reply[4..8].try_into().unwrap()), 0);

    let params = vec![(FieldId::NewsPath, b"Missing".as_ref())];
    let payload = encode_params(&params);
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: TransactionType::NewsArticleNameList.into(),
        id: 6,
        error: 0,
        total_size: payload.len() as u32,
        data_size: payload.len() as u32,
    };
    let tx = Transaction { header, payload };
    stream.write_all(&tx.to_bytes())?;

    let mut hdr_buf = [0u8; 20];
    stream.read_exact(&mut hdr_buf)?;
    let hdr = FrameHeader::from_bytes(&hdr_buf);
    assert_eq!(hdr.error, NEWS_ERR_PATH_UNSUPPORTED);
    Ok(())
}
