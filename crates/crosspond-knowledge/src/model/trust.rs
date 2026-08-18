use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustLevel {
    User,
    Reviewed,
    Derived,
    External,
}

impl TrustLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Reviewed => "reviewed",
            Self::Derived => "derived",
            Self::External => "external",
        }
    }
}
