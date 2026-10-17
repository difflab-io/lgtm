//! Posting state with an error only when the result is uncertain.
use super::super::{
    database::types::PostingState,
    error::{Result, invalid},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PostingStatus {
    Unposted,
    Sending,
    Uncertain { error: String },
    Confirmed,
}

impl PostingStatus {
    pub fn state(&self) -> PostingState {
        match self {
            Self::Unposted => PostingState::Unposted,
            Self::Sending => PostingState::Sending,
            Self::Uncertain { .. } => PostingState::Uncertain,
            Self::Confirmed => PostingState::Confirmed,
        }
    }

    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Uncertain { error } => Some(error),
            _ => None,
        }
    }

    pub fn from_db(state: PostingState, error: Option<String>) -> Result<Self> {
        match (state, error) {
            (PostingState::Unposted, None) => Ok(Self::Unposted),
            (PostingState::Sending, None) => Ok(Self::Sending),
            (PostingState::Confirmed, None) => Ok(Self::Confirmed),
            (PostingState::Uncertain, Some(error)) if !error.trim().is_empty() => {
                Ok(Self::Uncertain { error })
            }
            _ => Err(invalid("Posting state and error do not agree")),
        }
    }
}
