//! Utility to create AFL fuzzing corpus data.
//!
//! Generates a set of minimal transactions for the protocol fuzz target and
//! writes them into the `fuzz/corpus` directory.

#![expect(clippy::big_endian_bytes, reason = "network protocol uses big-endian")]

use std::io::Write;

use anyhow::{Context, Result};
use camino::Utf8Path;
use cap_std::fs_utf8::Dir;
use mxd::{
    field_id::FieldId,
    transaction::{FrameHeader, Transaction, encode_params},
    transaction_type::TransactionType,
};

const CORPUS_DIR: &str = "fuzz/corpus";

fn payload_tx(ty: TransactionType, id: u32, params: &[(FieldId, &[u8])]) -> Result<Transaction> {
    let payload = encode_params(params).context("encode corpus params")?;
    let payload_len = u32::try_from(payload.len()).context("payload length exceeds u32")?;
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: ty.into(),
        id,
        error: 0,
        total_size: payload_len,
        data_size: payload_len,
    };
    Ok(Transaction { header, payload })
}

fn login_tx() -> Result<Transaction> {
    let params = [
        (FieldId::Login, b"alice".as_ref()),
        (FieldId::Password, b"secret".as_ref()),
    ];
    payload_tx(TransactionType::Login, 1, &params)
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

fn news_category_root_tx() -> Result<Transaction> {
    let params = [(FieldId::NewsPath, b"/".as_ref())];
    payload_tx(TransactionType::NewsCategoryNameList, 3, &params)
}

fn news_article_titles_tx() -> Result<Transaction> {
    let params = [(FieldId::NewsPath, b"General".as_ref())];
    payload_tx(TransactionType::NewsArticleNameList, 7, &params)
}

fn news_article_data_tx() -> Result<Transaction> {
    let id_bytes = 1i32.to_be_bytes();
    let params = [
        (FieldId::NewsPath, b"General".as_ref()),
        (FieldId::NewsArticleId, id_bytes.as_ref()),
        (FieldId::NewsDataFlavor, b"text/plain".as_ref()),
    ];
    payload_tx(TransactionType::NewsArticleData, 8, &params)
}

fn save_tx(dir: &Dir, name: &Utf8Path, tx: &Transaction) -> Result<()> {
    let bytes = tx.to_bytes();
    let mut f = dir
        .create(name)
        .with_context(|| format!("create corpus file {name}"))?;
    f.write_all(&bytes).context("write corpus file")
}

fn main() -> Result<()> {
    Dir::create_ambient_dir_all(CORPUS_DIR, cap_std::ambient_authority())
        .context("create corpus directory")?;
    let dir = Dir::open_ambient_dir(CORPUS_DIR, cap_std::ambient_authority())
        .context("open corpus directory")?;

    let login = login_tx()?;
    save_tx(&dir, Utf8Path::new("login.bin"), &login)?;
    let list = get_file_list_tx();
    save_tx(&dir, Utf8Path::new("get_file_list.bin"), &list)?;
    let root = news_category_root_tx()?;
    save_tx(&dir, Utf8Path::new("news_category_root.bin"), &root)?;
    let titles = news_article_titles_tx()?;
    save_tx(&dir, Utf8Path::new("news_article_titles.bin"), &titles)?;
    let data = news_article_data_tx()?;
    save_tx(&dir, Utf8Path::new("news_article_data.bin"), &data)?;
    Ok(())
}
