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
            Self::UserAccess => f.write_str("UserAccess"),
            Self::NewsCategoryNameList => f.write_str("NewsCategoryNameList"),
            Self::NewsArticleNameList => f.write_str("NewsArticleNameList"),
            Self::NewsArticleData => f.write_str("NewsArticleData"),
            Self::PostNewsArticle => f.write_str("PostNewsArticle"),
            Self::Other(v) => write!(f, "Other({v})"),
        }
    }
}
