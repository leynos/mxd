//! Utility to create AFL fuzzing corpus data.
//!
//! Generates a set of minimal transactions for the protocol fuzz target and
//! writes them into the `fuzz/corpus` directory.
use std::{
    fs::{self, File},
    io::Write,
    path::Path,
};

use mxd::{
    field_id::FieldId,
    transaction::{FrameHeader, Transaction, encode_params},
    transaction_type::TransactionType,
};

const CORPUS_DIR: &str = "fuzz/corpus";

fn login_tx() -> Transaction {
    let params = [
        (FieldId::Login, b"alice".as_ref()),
        (FieldId::Password, b"secret".as_ref()),
    ];
    let payload = encode_params(&params).expect("encode");
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: TransactionType::Login.into(),
        id: 1,
        error: 0,
        total_size: u32::try_from(payload.len()).expect("payload fits in u32"),
        data_size: u32::try_from(payload.len()).expect("payload fits in u32"),
    };
    Transaction { header, payload }
}

fn get_file_list_tx() -> Transaction {
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: TransactionType::GetFileNameList.into(),
        id: 2,
        error: 0,
        total_size: 0,
        data_size: 0,
    };
    Transaction {
        header,
        payload: Vec::new(),
    }
}

fn news_category_root_tx() -> Transaction {
    let params = [(FieldId::NewsPath, b"/".as_ref())];
    let payload = encode_params(&params).expect("encode");
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: TransactionType::NewsCategoryNameList.into(),
        id: 3,
        error: 0,
        total_size: u32::try_from(payload.len()).expect("payload fits in u32"),
        data_size: u32::try_from(payload.len()).expect("payload fits in u32"),
    };
    Transaction { header, payload }
}

fn news_article_titles_tx() -> Transaction {
    let params = [(FieldId::NewsPath, b"General".as_ref())];
    let payload = encode_params(&params).expect("encode");
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: TransactionType::NewsArticleNameList.into(),
        id: 7,
        error: 0,
        total_size: u32::try_from(payload.len()).expect("payload fits in u32"),
        data_size: u32::try_from(payload.len()).expect("payload fits in u32"),
    };
    Transaction { header, payload }
}

fn news_article_data_tx() -> Transaction {
    let id_bytes = 1i32.to_be_bytes();
    let params = [
        (FieldId::NewsPath, b"General".as_ref()),
        (FieldId::NewsArticleId, id_bytes.as_ref()),
        (FieldId::NewsDataFlavor, b"text/plain".as_ref()),
    ];
    let payload = encode_params(&params).expect("encode");
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: TransactionType::NewsArticleData.into(),
        id: 8,
        error: 0,
        total_size: u32::try_from(payload.len()).expect("payload fits in u32"),
        data_size: u32::try_from(payload.len()).expect("payload fits in u32"),
    };
    Transaction { header, payload }
}

fn save_tx(tx: &Transaction, path: &Path) -> std::io::Result<()> {
    let bytes = tx.to_bytes();
    let mut f = File::create(path)?;
    f.write_all(&bytes)?;
    Ok(())
}

fn main() -> std::io::Result<()> {
    fs::create_dir_all(CORPUS_DIR)?;
    let dir = Path::new(CORPUS_DIR);
    let login = login_tx();
    save_tx(&login, &dir.join("login.bin"))?;
    let list = get_file_list_tx();
    save_tx(&list, &dir.join("get_file_list.bin"))?;
    let root = news_category_root_tx();
    save_tx(&root, &dir.join("news_category_root.bin"))?;
    let titles = news_article_titles_tx();
    save_tx(&titles, &dir.join("news_article_titles.bin"))?;
    let data = news_article_data_tx();
    save_tx(&data, &dir.join("news_article_data.bin"))?;
    Ok(())
}
