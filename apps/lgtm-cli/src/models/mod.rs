//! CLI view models transforming backend Protobuf payloads into terminal and JSON views.

pub mod comment;
pub mod common;
pub mod git;
pub mod review;

pub use comment::{CommentView, DeletedCommentView};
pub use common::{
    ActorArgs, LocalArgs, ReviewGroupBy, ReviewOutcome, parse_comment_id, parse_nonempty,
};
pub use git::{AddView, CommitView, PushView};
pub use review::{ReviewDetailView, ReviewListView, ReviewRefreshView, ReviewSessionView};
