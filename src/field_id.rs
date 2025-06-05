#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FieldId {
    /// Login name for an account.
    Login,
    /// Password for an account.
    Password,
    /// Client version information.
    Version,
    /// Any other field id not explicitly covered.
    Other(u16),
}

impl From<u16> for FieldId {
    fn from(v: u16) -> Self {
        match v {
            105 => Self::Login,
            106 => Self::Password,
            160 => Self::Version,
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
            FieldId::Other(v) => write!(f, "Other({v})"),
        }
    }
}
