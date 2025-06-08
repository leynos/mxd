use std::fs::{self, File};
use std::io::Write;

use mxd::field_id::FieldId;
use mxd::transaction::{FrameHeader, Transaction, encode_params};
use mxd::transaction_type::TransactionType;

fn login_tx() -> Transaction {
    let params = [
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
    let payload = encode_params(&params);
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: TransactionType::NewsCategoryNameList.into(),
        id: 3,
        error: 0,
        total_size: payload.len() as u32,
        data_size: payload.len() as u32,
    };
    Transaction { header, payload }
}

fn news_article_titles_tx() -> Transaction {
    let params = [(FieldId::NewsPath, b"General".as_ref())];
    let payload = encode_params(&params);
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: TransactionType::NewsArticleNameList.into(),
        id: 7,
        error: 0,
        total_size: payload.len() as u32,
        data_size: payload.len() as u32,
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
    let payload = encode_params(&params);
    let header = FrameHeader {
        flags: 0,
        is_reply: 0,
        ty: TransactionType::NewsArticleData.into(),
        id: 8,
        error: 0,
        total_size: payload.len() as u32,
        data_size: payload.len() as u32,
    };
    Transaction { header, payload }
}

fn save_tx(tx: Transaction, path: &str) -> std::io::Result<()> {
    let bytes = tx.to_bytes();
    let mut f = File::create(path)?;
    f.write_all(&bytes)?;
    Ok(())
}

fn main() -> std::io::Result<()> {
    fs::create_dir_all("fuzz/corpus")?;
    save_tx(login_tx(), "fuzz/corpus/login.bin")?;
    save_tx(get_file_list_tx(), "fuzz/corpus/get_file_list.bin")?;
    save_tx(
        news_category_root_tx(),
        "fuzz/corpus/news_category_root.bin",
    )?;
    save_tx(
        news_article_titles_tx(),
        "fuzz/corpus/news_article_titles.bin",
    )?;
    save_tx(news_article_data_tx(), "fuzz/corpus/news_article_data.bin")?;
    Ok(())
}
