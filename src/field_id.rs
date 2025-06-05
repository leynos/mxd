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
