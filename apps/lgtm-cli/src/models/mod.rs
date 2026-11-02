//! CLI view models transforming backend Protobuf payloads into terminal and JSON outputs.

pub mod command_output;
pub mod comment;
pub mod common;
pub mod diff;
pub mod review;

pub use command_output::{AddCommandOutput, CommitCommandOutput, PushCommandOutput};
pub use comment::{Comment, DeleteCommentCommandOutput};
pub use common::{
    ActorArgs, LocalArgs, ReviewGroupBy, ReviewOutcome, parse_comment_id, parse_nonempty,
};
pub use review::{
    ListReviewsCommandOutput, RefreshReviewCommandOutput, ReviewSessionCommandOutput,
    ShowReviewCommandOutput,
};
