#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionType {
    Error,
    Login,
    Agreement,
    Agreed,
    UserAccess,
    NewsCategoryList,
    Other(u16),
}

impl From<u16> for TransactionType {
    fn from(v: u16) -> Self {
        match v {
            100 => Self::Error,
            107 => Self::Login,
            109 => Self::Agreement,
            121 => Self::Agreed,
            354 => Self::UserAccess,
            370 => Self::NewsCategoryList,
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
            TransactionType::UserAccess => 354,
            TransactionType::NewsCategoryList => 370,
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
            TransactionType::UserAccess => f.write_str("UserAccess"),
            TransactionType::NewsCategoryList => f.write_str("NewsCategoryList"),
            TransactionType::Other(v) => write!(f, "Other({v})"),
        }
    }
}
