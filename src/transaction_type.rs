//! Enumeration of supported transaction types.
//!
//! Each variant corresponds to a Hotline protocol transaction identifier used
//! for client/server communication.
/// Transaction type identifier for file name list requests.
pub const FILE_NAME_LIST_ID: u16 = 200;
/// Transaction type identifier for banner download requests.
pub const DOWNLOAD_BANNER_ID: u16 = 212;
/// Transaction type identifier for user name list requests.
pub const USER_NAME_LIST_ID: u16 = 300;
/// Transaction type identifier for notify-change-user transactions.
pub const NOTIFY_CHANGE_USER_ID: u16 = 301;
/// Transaction type identifier for notify-delete-user transactions.
pub const NOTIFY_DELETE_USER_ID: u16 = 302;
/// Transaction type identifier for get-client-info transactions.
pub const GET_CLIENT_INFO_TEXT_ID: u16 = 303;
/// Transaction type identifier for set-client-user-info transactions.
pub const SET_CLIENT_USER_INFO_ID: u16 = 304;

/// Transaction types supported by the Hotline protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionType {
    /// Server error response.
    Error,
    /// User login request.
    Login,
    /// Server agreement/banner display.
    Agreement,
    /// Client has accepted the agreement.
    Agreed,
    /// Request for the list of available files.
    GetFileNameList,
    /// Request to download the server's banner image.
    DownloadBanner,
    /// Request the list of logged-in users.
    GetUserNameList,
    /// Server notification that a user's public data changed.
    NotifyChangeUser,
    /// Server notification that a user left the online roster.
    NotifyDeleteUser,
    /// Request another user's public info text.
    GetClientInfoText,
    /// Update the current session's public user info.
    SetClientUserInfo,
    /// User access privileges response.
    UserAccess,
    /// Request for news category names.
    NewsCategoryNameList,
    /// Request for news article names within a category.
    NewsArticleNameList,
    /// Request for a specific news article's content.
    NewsArticleData,
    /// Request to post a new news article.
    PostNewsArticle,
    /// Any other transaction type not explicitly handled.
    Other(u16),
}

impl TransactionType {
    /// Return true if this transaction type may include a payload.
    #[must_use]
    pub const fn allows_payload(self) -> bool {
        !matches!(
            self,
            Self::GetFileNameList | Self::DownloadBanner | Self::GetUserNameList
        )
    }

    /// Return `true` when a non-empty payload should be rejected outright.
    #[must_use]
    pub const fn rejects_payload(self, payload_is_empty: bool) -> bool {
        if payload_is_empty {
            return false;
        }
        match self {
            // SynHX sends a binary `DATA_DIR` block for `/ls`, even for the
            // root listing flow that MXD currently treats as a single logical
            // file-list command. Accept the payload and let the handler ignore
            // it until directory-aware semantics land.
            Self::GetFileNameList => false,
            _ => !self.allows_payload(),
        }
    }

    /// Return `true` when request payload bytes should bypass decode attempts.
    #[must_use]
    pub const fn bypass_payload_decode(self) -> bool {
        matches!(self, Self::GetFileNameList) || !self.allows_payload()
    }
}

impl From<u16> for TransactionType {
    fn from(v: u16) -> Self {
        match v {
            100 => Self::Error,
            107 => Self::Login,
            109 => Self::Agreement,
            121 => Self::Agreed,
            FILE_NAME_LIST_ID => Self::GetFileNameList,
            DOWNLOAD_BANNER_ID => Self::DownloadBanner,
            USER_NAME_LIST_ID => Self::GetUserNameList,
            NOTIFY_CHANGE_USER_ID => Self::NotifyChangeUser,
            NOTIFY_DELETE_USER_ID => Self::NotifyDeleteUser,
            GET_CLIENT_INFO_TEXT_ID => Self::GetClientInfoText,
            SET_CLIENT_USER_INFO_ID => Self::SetClientUserInfo,
            354 => Self::UserAccess,
            370 => Self::NewsCategoryNameList,
            371 => Self::NewsArticleNameList,
            400 => Self::NewsArticleData,
            410 => Self::PostNewsArticle,
            other => Self::Other(other),
        }
    }
}

impl From<TransactionType> for u16 {
    fn from(t: TransactionType) -> Self {
        match t {
            TransactionType::Error => 100,
            TransactionType::Login => 107,
            TransactionType::Agreement => 109,
            TransactionType::Agreed => 121,
            TransactionType::GetFileNameList => FILE_NAME_LIST_ID,
            TransactionType::DownloadBanner => DOWNLOAD_BANNER_ID,
            TransactionType::GetUserNameList => USER_NAME_LIST_ID,
            TransactionType::NotifyChangeUser => NOTIFY_CHANGE_USER_ID,
            TransactionType::NotifyDeleteUser => NOTIFY_DELETE_USER_ID,
            TransactionType::GetClientInfoText => GET_CLIENT_INFO_TEXT_ID,
            TransactionType::SetClientUserInfo => SET_CLIENT_USER_INFO_ID,
            TransactionType::UserAccess => 354,
            TransactionType::NewsCategoryNameList => 370,
            TransactionType::NewsArticleNameList => 371,
            TransactionType::NewsArticleData => 400,
            TransactionType::PostNewsArticle => 410,
            TransactionType::Other(v) => v,
        }
    }
}

impl std::fmt::Display for TransactionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Error => f.write_str("Error"),
            Self::Login => f.write_str("Login"),
            Self::Agreement => f.write_str("Agreement"),
            Self::Agreed => f.write_str("Agreed"),
            Self::GetFileNameList => f.write_str("GetFileNameList"),
            Self::DownloadBanner => f.write_str("DownloadBanner"),
            Self::GetUserNameList => f.write_str("GetUserNameList"),
            Self::NotifyChangeUser => f.write_str("NotifyChangeUser"),
            Self::NotifyDeleteUser => f.write_str("NotifyDeleteUser"),
            Self::GetClientInfoText => f.write_str("GetClientInfoText"),
            Self::SetClientUserInfo => f.write_str("SetClientUserInfo"),
            Self::UserAccess => f.write_str("UserAccess"),
            Self::NewsCategoryNameList => f.write_str("NewsCategoryNameList"),
            Self::NewsArticleNameList => f.write_str("NewsArticleNameList"),
            Self::NewsArticleData => f.write_str("NewsArticleData"),
            Self::PostNewsArticle => f.write_str("PostNewsArticle"),
            Self::Other(v) => write!(f, "Other({v})"),
        }
    }
}

#[cfg(test)]
mod tests {
    //! Tests for `TransactionType` payload-policy helpers across explicit and
    //! table-driven cases.

    use rstest::rstest;

    use super::TransactionType;

    const ALL_TRANSACTION_TYPES: [TransactionType; 13] = [
        TransactionType::Error,
        TransactionType::Login,
        TransactionType::Agreement,
        TransactionType::Agreed,
        TransactionType::GetFileNameList,
        TransactionType::DownloadBanner,
        TransactionType::GetUserNameList,
        TransactionType::UserAccess,
        TransactionType::NewsCategoryNameList,
        TransactionType::NewsArticleNameList,
        TransactionType::NewsArticleData,
        TransactionType::PostNewsArticle,
        TransactionType::Other(999),
    ];

    #[rstest]
    #[case(TransactionType::GetFileNameList, false, false)]
    #[case(TransactionType::NewsArticleData, false, false)]
    #[case(TransactionType::DownloadBanner, false, true)]
    #[case(TransactionType::GetUserNameList, false, true)]
    fn rejects_payload_matches_expected_policy(
        #[case] transaction_type: TransactionType,
        #[case] expected_for_empty_payload: bool,
        #[case] expected_for_non_empty_payload: bool,
    ) {
        assert_eq!(
            transaction_type.rejects_payload(true),
            expected_for_empty_payload
        );
        assert_eq!(
            transaction_type.rejects_payload(false),
            expected_for_non_empty_payload
        );
    }

    #[rstest]
    #[case(TransactionType::Error, false)]
    #[case(TransactionType::Login, false)]
    #[case(TransactionType::Agreement, false)]
    #[case(TransactionType::Agreed, false)]
    #[case(TransactionType::GetFileNameList, true)]
    #[case(TransactionType::DownloadBanner, true)]
    #[case(TransactionType::GetUserNameList, true)]
    #[case(TransactionType::UserAccess, false)]
    #[case(TransactionType::NewsCategoryNameList, false)]
    #[case(TransactionType::NewsArticleNameList, false)]
    #[case(TransactionType::NewsArticleData, false)]
    #[case(TransactionType::PostNewsArticle, false)]
    #[case(TransactionType::Other(999), false)]
    fn bypass_payload_decode_matches_transaction_policy(
        #[case] transaction_type: TransactionType,
        #[case] expected: bool,
    ) {
        assert!(
            ALL_TRANSACTION_TYPES.contains(&transaction_type),
            "missing coverage entry for {transaction_type:?}"
        );
        assert_eq!(
            transaction_type.bypass_payload_decode(),
            expected,
            "unexpected bypass policy for {transaction_type:?}"
        );
    }
}
