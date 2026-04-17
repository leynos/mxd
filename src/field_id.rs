//! Field identifiers used within transaction payloads.
//!
//! Each `FieldId` corresponds to a specific parameter or data value defined by
//! the Hotline protocol. They are used when encoding and decoding transaction
//! parameters.
/// Field identifiers for transaction parameters.
///
/// Each variant represents a specific parameter type used in the Hotline
/// protocol's transaction payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FieldId {
    /// User-visible nickname.
    Name,
    /// Login name for an account.
    Login,
    /// Password for an account.
    Password,
    /// User identifier.
    UserId,
    /// User icon identifier.
    IconId,
    /// User access bitmap.
    UserAccess,
    /// User list colour or status flags.
    UserFlags,
    /// Connection option flags.
    Options,
    /// Main chat subject.
    ChatSubject,
    /// Client version information.
    Version,
    /// Banner identifier used for HTTP banner retrieval.
    BannerId,
    /// Server name string returned during login.
    ServerName,
    /// Generic data payload (often message text).
    Data,
    /// News category list entry returned by the server.
    NewsCategory,
    /// News article list entry returned by the server.
    NewsArticle,
    /// Article identifier in requests.
    NewsArticleId,
    /// Data flavor for article content.
    NewsDataFlavor,
    /// Article title field.
    NewsTitle,
    /// Article poster field.
    NewsPoster,
    /// Article post date.
    NewsDate,
    /// Previous article id for navigation.
    NewsPrevId,
    /// Next article id for navigation.
    NewsNextId,
    /// Article data payload.
    NewsArticleData,
    /// Article flags field (field 334).
    ///
    /// Protocol semantics: indicates whether a post is locked or is an
    /// announcement type. Typically 0 for normal posts. Flag values are not
    /// strictly defined in the protocol specification; implementations may
    /// vary.
    NewsArticleFlags,
    /// Parent article id field.
    NewsParentId,
    /// First child article id field.
    NewsFirstChildId,
    /// Path within the news hierarchy.
    NewsPath,
    /// File name entry.
    FileName,
    /// Packed user-list entry containing id, icon, flags, and name.
    UserNameWithInfo,
    /// Automatic response text.
    AutoResponse,
    /// Any other field id not explicitly covered.
    Other(u16),
}

impl From<u16> for FieldId {
    fn from(v: u16) -> Self {
        match v {
            101 => Self::Data,
            102 => Self::Name,
            103 => Self::UserId,
            104 => Self::IconId,
            105 => Self::Login,
            106 => Self::Password,
            110 => Self::UserAccess,
            112 => Self::UserFlags,
            113 => Self::Options,
            115 => Self::ChatSubject,
            160 => Self::Version,
            161 => Self::BannerId,
            162 => Self::ServerName,
            215 => Self::AutoResponse,
            323 => Self::NewsCategory,
            321 => Self::NewsArticle,
            326 => Self::NewsArticleId,
            327 => Self::NewsDataFlavor,
            328 => Self::NewsTitle,
            329 => Self::NewsPoster,
            330 => Self::NewsDate,
            331 => Self::NewsPrevId,
            332 => Self::NewsNextId,
            333 => Self::NewsArticleData,
            334 => Self::NewsArticleFlags,
            335 => Self::NewsParentId,
            336 => Self::NewsFirstChildId,
            325 => Self::NewsPath,
            crate::transaction_type::FILE_NAME_LIST_ID => Self::FileName,
            crate::transaction_type::USER_NAME_LIST_ID => Self::UserNameWithInfo,
            other => Self::Other(other),
        }
    }
}

impl From<FieldId> for u16 {
    fn from(f: FieldId) -> Self {
        match f {
            FieldId::Name => 102,
            FieldId::Login => 105,
            FieldId::Password => 106,
            FieldId::UserId => 103,
            FieldId::IconId => 104,
            FieldId::UserAccess => 110,
            FieldId::UserFlags => 112,
            FieldId::Options => 113,
            FieldId::ChatSubject => 115,
            FieldId::Version => 160,
            FieldId::BannerId => 161,
            FieldId::ServerName => 162,
            FieldId::Data => 101,
            FieldId::AutoResponse => 215,
            FieldId::NewsCategory => 323,
            FieldId::NewsArticle => 321,
            FieldId::NewsArticleId => 326,
            FieldId::NewsDataFlavor => 327,
            FieldId::NewsTitle => 328,
            FieldId::NewsPoster => 329,
            FieldId::NewsDate => 330,
            FieldId::NewsPrevId => 331,
            FieldId::NewsNextId => 332,
            FieldId::NewsArticleData => 333,
            FieldId::NewsArticleFlags => 334,
            FieldId::NewsParentId => 335,
            FieldId::NewsFirstChildId => 336,
            FieldId::NewsPath => 325,
            FieldId::FileName => crate::transaction_type::FILE_NAME_LIST_ID,
            FieldId::UserNameWithInfo => crate::transaction_type::USER_NAME_LIST_ID,
            FieldId::Other(v) => v,
        }
    }
}

impl std::fmt::Display for FieldId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Name => f.write_str("Name"),
            Self::Login => f.write_str("Login"),
            Self::Password => f.write_str("Password"),
            Self::UserId => f.write_str("UserId"),
            Self::IconId => f.write_str("IconId"),
            Self::UserAccess => f.write_str("UserAccess"),
            Self::UserFlags => f.write_str("UserFlags"),
            Self::Options => f.write_str("Options"),
            Self::ChatSubject => f.write_str("ChatSubject"),
            Self::Version => f.write_str("Version"),
            Self::BannerId => f.write_str("BannerId"),
            Self::ServerName => f.write_str("ServerName"),
            Self::Data => f.write_str("Data"),
            Self::AutoResponse => f.write_str("AutoResponse"),
            Self::NewsCategory => f.write_str("NewsCategory"),
            Self::NewsArticle => f.write_str("NewsArticle"),
            Self::NewsArticleId => f.write_str("NewsArticleId"),
            Self::NewsDataFlavor => f.write_str("NewsDataFlavor"),
            Self::NewsTitle => f.write_str("NewsTitle"),
            Self::NewsPoster => f.write_str("NewsPoster"),
            Self::NewsDate => f.write_str("NewsDate"),
            Self::NewsPrevId => f.write_str("NewsPrevId"),
            Self::NewsNextId => f.write_str("NewsNextId"),
            Self::NewsArticleData => f.write_str("NewsArticleData"),
            Self::NewsArticleFlags => f.write_str("NewsArticleFlags"),
            Self::NewsParentId => f.write_str("NewsParentId"),
            Self::NewsFirstChildId => f.write_str("NewsFirstChildId"),
            Self::NewsPath => f.write_str("NewsPath"),
            Self::FileName => f.write_str("FileName"),
            Self::UserNameWithInfo => f.write_str("UserNameWithInfo"),
            Self::Other(v) => write!(f, "Other({v})"),
        }
    }
}
