//! LGTM persistence layer for identities, reviews, sessions, and comments.

pub mod client;
pub(crate) mod entities;
pub mod error;
pub(crate) mod migration;
pub mod repositories;

pub use client::{Database, Transaction};
pub use entities::{
    comment::Model as Comment,
    common::{IdentityKind, ReviewKind, ReviewOutcome, ReviewStatus, SessionState},
    identity::Model as Identity,
    review::Model as Review,
    session::Model as Session,
};
pub use error::{DbError, Result};
pub use repositories::{
    CommentAnchorShift, CommentsRepository, IdentitiesRepository, NewComment, NewLocalReview,
    ReviewFilter, ReviewsRepository, SessionsRepository, UpdateComment, UpsertForgeComment,
    UpsertForgeReview, UpsertForgeSession,
};
