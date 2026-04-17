//! Transaction-to-command parsing helpers.

use super::{Command, UserInfoUpdate};
use crate::{
    connection_flags::ConnectionFlags,
    field_id::FieldId,
    login::LoginRequest,
    news_handlers::PostArticleRequest,
    transaction::{
        FrameHeader,
        Transaction,
        TransactionError,
        decode_params_map,
        first_param_i32,
        first_param_string,
        first_param_u32,
        required_param_i32,
        required_param_string,
        required_param_u32,
    },
    transaction_type::TransactionType,
};

/// Parsed login credentials extracted from transaction parameters.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct LoginCredentials {
    /// Username for authentication.
    pub(super) username: String,
    /// Password for authentication.
    pub(super) password: String,
}

/// Extract username and password from login payload parameters.
pub(super) fn parse_login_params(payload: &[u8]) -> Result<LoginCredentials, TransactionError> {
    let params = decode_params_map(payload)?;
    Ok(LoginCredentials {
        username: required_param_string(&params, FieldId::Login)?,
        password: required_param_string(&params, FieldId::Password)?,
    })
}

/// Convert a parsed transaction into a high-level command.
pub(super) fn parse_command(tx: Transaction) -> Result<Command, TransactionError> {
    let ty = TransactionType::from(tx.header.ty);
    if !ty.allows_payload() && !tx.payload.is_empty() {
        return Ok(Command::InvalidPayload { header: tx.header });
    }
    match ty {
        TransactionType::Login => {
            let creds = parse_login_params(&tx.payload)?;
            Ok(Command::Login {
                req: LoginRequest {
                    username: creds.username,
                    password: creds.password,
                    header: tx.header,
                },
            })
        }
        TransactionType::GetUserNameList => Ok(Command::GetUserNameList { header: tx.header }),
        TransactionType::GetClientInfoText => {
            parse_get_client_info_text_params(&tx.payload, tx.header)
        }
        TransactionType::SetClientUserInfo => {
            parse_set_client_user_info_params(&tx.payload, tx.header)
        }
        TransactionType::GetFileNameList => Ok(Command::GetFileNameList { header: tx.header }),
        TransactionType::NewsCategoryNameList => {
            parse_news_category_name_list_params(&tx.payload, tx.header)
        }
        TransactionType::NewsArticleNameList => {
            parse_news_article_name_list_params(&tx.payload, tx.header)
        }
        TransactionType::NewsArticleData => parse_news_article_data_params(&tx.payload, tx.header),
        TransactionType::PostNewsArticle => parse_post_news_article_params(&tx.payload, tx.header),
        _ => Ok(Command::Unknown { header: tx.header }),
    }
}

fn parse_news_category_name_list_params(
    payload: &[u8],
    header: FrameHeader,
) -> Result<Command, TransactionError> {
    let params = decode_params_map(payload)?;
    let path = first_param_string(&params, FieldId::NewsPath)?;
    Ok(Command::GetNewsCategoryNameList { path, header })
}

fn parse_news_article_name_list_params(
    payload: &[u8],
    header: FrameHeader,
) -> Result<Command, TransactionError> {
    let params = decode_params_map(payload)?;
    let path = required_param_string(&params, FieldId::NewsPath)?;
    Ok(Command::GetNewsArticleNameList { path, header })
}

fn parse_news_article_data_params(
    payload: &[u8],
    header: FrameHeader,
) -> Result<Command, TransactionError> {
    let params = decode_params_map(payload)?;
    let path = required_param_string(&params, FieldId::NewsPath)?;
    let article_id = required_param_i32(&params, FieldId::NewsArticleId)?;
    Ok(Command::GetNewsArticleData {
        path,
        article_id,
        header,
    })
}

fn parse_get_client_info_text_params(
    payload: &[u8],
    header: FrameHeader,
) -> Result<Command, TransactionError> {
    let params = decode_params_map(payload)?;
    let target_user_id = i32::try_from(required_param_u32(&params, FieldId::UserId)?)
        .map_err(|_| TransactionError::InvalidParamValue(FieldId::UserId))?;
    Ok(Command::GetClientInfoText {
        header,
        target_user_id,
    })
}

fn parse_set_client_user_info_params(
    payload: &[u8],
    header: FrameHeader,
) -> Result<Command, TransactionError> {
    let params = decode_params_map(payload)?;
    let display_name = first_param_string(&params, FieldId::Name)?;
    let icon_id = first_param_u32(&params, FieldId::IconId)?
        .map(u16::try_from)
        .transpose()
        .map_err(|_| TransactionError::InvalidParamValue(FieldId::IconId))?;
    let options = first_param_u32(&params, FieldId::Options)?
        .map(u8::try_from)
        .transpose()
        .map_err(|_| TransactionError::InvalidParamValue(FieldId::Options))?
        .map(ConnectionFlags::from_bits_truncate);
    let auto_response = first_param_string(&params, FieldId::AutoResponse)?;
    Ok(Command::SetClientUserInfo {
        header,
        update: UserInfoUpdate {
            display_name,
            icon_id,
            options,
            auto_response,
        },
    })
}

fn parse_post_news_article_params(
    payload: &[u8],
    header: FrameHeader,
) -> Result<Command, TransactionError> {
    let params = decode_params_map(payload)?;
    let path = required_param_string(&params, FieldId::NewsPath)?;
    let title = required_param_string(&params, FieldId::NewsTitle)?;
    let flags = first_param_i32(&params, FieldId::NewsArticleFlags)?.unwrap_or(0);
    let data_flavor = required_param_string(&params, FieldId::NewsDataFlavor)?;
    let data = required_param_string(&params, FieldId::NewsArticleData)?;
    Ok(Command::PostNewsArticle {
        req: PostArticleRequest {
            path,
            title,
            flags,
            data_flavor,
            data,
        },
        header,
    })
}
