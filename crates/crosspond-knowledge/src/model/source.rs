use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceStatus {
    Unread,
    Processing,
    Processed,
    Archived,
}

impl SourceStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unread => "unread",
            Self::Processing => "processing",
            Self::Processed => "processed",
            Self::Archived => "archived",
        }
    }
}
