#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FieldId {
    /// Login name for an account.
    Login,
    /// Password for an account.
    Password,
    /// Client version information.
    Version,
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
    /// Article flags field.
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
            105 => Self::Login,
            106 => Self::Password,
            160 => Self::Version,
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
            FieldId::Login => f.write_str("Login"),
            FieldId::Password => f.write_str("Password"),
            FieldId::Version => f.write_str("Version"),
            FieldId::NewsCategory => f.write_str("NewsCategory"),
            FieldId::NewsArticle => f.write_str("NewsArticle"),
            FieldId::NewsArticleId => f.write_str("NewsArticleId"),
            FieldId::NewsDataFlavor => f.write_str("NewsDataFlavor"),
            FieldId::NewsTitle => f.write_str("NewsTitle"),
            FieldId::NewsPoster => f.write_str("NewsPoster"),
            FieldId::NewsDate => f.write_str("NewsDate"),
            FieldId::NewsPrevId => f.write_str("NewsPrevId"),
            FieldId::NewsNextId => f.write_str("NewsNextId"),
            FieldId::NewsArticleData => f.write_str("NewsArticleData"),
            FieldId::NewsArticleFlags => f.write_str("NewsArticleFlags"),
            FieldId::NewsParentId => f.write_str("NewsParentId"),
            FieldId::NewsFirstChildId => f.write_str("NewsFirstChildId"),
            FieldId::NewsPath => f.write_str("NewsPath"),
            FieldId::FileName => f.write_str("FileName"),
            FieldId::Other(v) => write!(f, "Other({v})"),
        }
    }
}
