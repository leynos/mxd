pub const FILE_NAME_LIST_ID: u16 = 200;
pub const DOWNLOAD_BANNER_ID: u16 = 212;
pub const USER_NAME_LIST_ID: u16 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionType {
    Error,
    Login,
    Agreement,
    Agreed,
    GetFileNameList,
    /// Request to download the server's banner image.
    DownloadBanner,
    /// Request the list of logged-in users.
    GetUserNameList,
    UserAccess,
    NewsCategoryNameList,
    NewsArticleNameList,
    NewsArticleData,
    PostNewsArticle,
    Other(u16),
}

impl TransactionType {
    /// Return true if this transaction type may include a payload.
    pub fn allows_payload(self) -> bool {
        !matches!(
            self,
            TransactionType::GetFileNameList
                | TransactionType::DownloadBanner
                | TransactionType::GetUserNameList
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
            TransactionType::Error => f.write_str("Error"),
            TransactionType::Login => f.write_str("Login"),
            TransactionType::Agreement => f.write_str("Agreement"),
            TransactionType::Agreed => f.write_str("Agreed"),
            TransactionType::GetFileNameList => f.write_str("GetFileNameList"),
            TransactionType::DownloadBanner => f.write_str("DownloadBanner"),
            TransactionType::GetUserNameList => f.write_str("GetUserNameList"),
            TransactionType::UserAccess => f.write_str("UserAccess"),
            TransactionType::NewsCategoryNameList => f.write_str("NewsCategoryNameList"),
            TransactionType::NewsArticleNameList => f.write_str("NewsArticleNameList"),
            TransactionType::NewsArticleData => f.write_str("NewsArticleData"),
            TransactionType::PostNewsArticle => f.write_str("PostNewsArticle"),
            TransactionType::Other(v) => write!(f, "Other({v})"),
        }
    }
}
