//! Public repository structs and input/patch contracts.

pub mod comments;
pub mod identities;
pub mod reviews;
pub mod sessions;

use crate::error::{DbError, Result};

pub use comments::{
    CommentAnchorShift, CommentsRepository, NewComment, UpdateComment, UpsertForgeComment,
};
pub use identities::IdentitiesRepository;
pub use reviews::{NewLocalReview, ReviewFilter, ReviewsRepository, UpsertForgeReview};
pub use sessions::{SessionsRepository, UpsertForgeSession};

// Private ---------------------------------------------------------------------

pub(super) fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
}

pub(super) fn require_non_empty<'a>(field: &'static str, value: &'a str) -> Result<&'a str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(DbError::invalid_input(format!("{field} must not be empty")));
    }
    Ok(trimmed)
}
