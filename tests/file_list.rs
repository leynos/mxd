use std::io::{Read, Write};
use std::net::TcpStream;

use argon2::Argon2;
use diesel_async::AsyncConnection;
use mxd::db::{DbConnection, add_file_acl, create_file, create_user, run_migrations};
use mxd::field_id::FieldId;
use mxd::models::{NewFileAcl, NewFileEntry, NewUser};
use mxd::transaction::{FrameHeader, Transaction, decode_params, encode_params};
use mxd::transaction_type::TransactionType;
use mxd::users::hash_password;
use test_util::TestServer;

#[test]
fn list_files_acl() -> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start_with_setup("./Cargo.toml", |db| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut conn = DbConnection::establish(db.to_str().unwrap()).await.unwrap();
            run_migrations(&mut conn).await.unwrap();
            let argon2 = Argon2::default();
            let hashed = hash_password(&argon2, "secret").unwrap();
            let new_user = NewUser {
                username: "alice",
                password: &hashed,
            };
            create_user(&mut conn, &new_user).await.unwrap();
            let files = [
                NewFileEntry {
                    name: "fileA.txt",
                    object_key: "1",
                    size: 1,
                },
                NewFileEntry {
                    name: "fileB.txt",
                    object_key: "2",
                    size: 1,
                },
                NewFileEntry {
                    name: "fileC.txt",
                    object_key: "3",
                    size: 1,
                },
            ];
            for file in &files {
                create_file(&mut conn, file).await.unwrap();
            }
            let acls = [
                NewFileAcl {
                    file_id: 1,
                    user_id: 1,
                },
                NewFileAcl {
                    file_id: 3,
                    user_id: 1,
                },
            ];
            for acl in &acls {
                add_file_acl(&mut conn, acl).await.unwrap();
            }
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

    // login
    let params = vec![
        (FieldId::Login, b"alice".as_ref()),
        (FieldId::Password, b"secret".as_ref()),
    ];
    let payload = encode_params(&params);
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: TransactionType::Login.into(),
        id: 1,
        error: 0,
        total_size: payload.len() as u32,
        data_size: payload.len() as u32,
    };
    let tx = Transaction { header, payload };
    stream.write_all(&tx.to_bytes())?;
    let mut buf = [0u8; 20];
    stream.read_exact(&mut buf)?;
    let reply_hdr = FrameHeader::from_bytes(&buf);
    let mut data = vec![0u8; reply_hdr.data_size as usize];
    stream.read_exact(&mut data)?;

    assert_eq!(reply_hdr.error, 0);

    // list files
    let payload = encode_params(&[]);
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: TransactionType::GetFileNameList.into(),
        id: 2,
        error: 0,
        total_size: payload.len() as u32,
        data_size: payload.len() as u32,
    };
    let tx = Transaction { header, payload };
    stream.write_all(&tx.to_bytes())?;
    stream.read_exact(&mut buf)?;
    let hdr = FrameHeader::from_bytes(&buf);
    let mut payload = vec![0u8; hdr.data_size as usize];
    stream.read_exact(&mut payload)?;
    let resp = Transaction {
        header: hdr,
        payload,
    };
    assert_eq!(resp.header.error, 0);
    let params = decode_params(&resp.payload)?;
    let names: Vec<String> = params
        .into_iter()
        .filter_map(|(id, d)| {
            if id == FieldId::FileNameWithInfo {
                Some(String::from_utf8(d).unwrap())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(names, vec!["fileA.txt", "fileC.txt"]);
    Ok(())
}
