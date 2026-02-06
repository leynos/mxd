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
    /// Login name for an account.
    Login,
    /// Password for an account.
    Password,
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
    /// Any other field id not explicitly covered.
    Other(u16),
}

impl From<u16> for FieldId {
    fn from(v: u16) -> Self {
        match v {
            101 => Self::Data,
            105 => Self::Login,
            106 => Self::Password,
            160 => Self::Version,
            161 => Self::BannerId,
            162 => Self::ServerName,
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
            other => Self::Other(other),
        }
    }
}

impl From<FieldId> for u16 {
    fn from(f: FieldId) -> Self {
        match f {
            FieldId::Login => 105,
            FieldId::Password => 106,
            FieldId::Version => 160,
            FieldId::BannerId => 161,
            FieldId::ServerName => 162,
            FieldId::Data => 101,
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
            FieldId::Other(v) => v,
        }
    }
}

impl std::fmt::Display for FieldId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Login => f.write_str("Login"),
            Self::Password => f.write_str("Password"),
            Self::Version => f.write_str("Version"),
            Self::BannerId => f.write_str("BannerId"),
            Self::ServerName => f.write_str("ServerName"),
            Self::Data => f.write_str("Data"),
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
            Self::Other(v) => write!(f, "Other({v})"),
        }
    }
}
