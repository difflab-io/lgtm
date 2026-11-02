//! Comment models and output types for `lgtm comment` commands and review threads.

use anyhow::Context;
use serde_json::{Value, json};

use crate::{
    backend::proto::lgtm::v1::{
        CommentDetail, CreateCommentResponse, DeleteCommentResponse, UpdateCommentResponse,
    },
    models::common::{CommentAnchor, Identity},
};

/// Single review comment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comment {
    pub id: String,
    pub number: u64,
    pub review_id: String,
    pub session_id: Option<String>,
    pub author: Option<Identity>,
    pub reply_to_number: Option<u64>,
    pub anchor: Option<CommentAnchor>,
    pub body: String,
    pub resolved: bool,
    pub outdated: bool,
    pub forge_id: Option<String>,
}

impl Comment {
    // Public ------------------------------------------------------------------

    /// Return the display label for the comment author.
    pub fn author_label(&self) -> String {
        self.author
            .as_ref()
            .map_or_else(|| "unknown".to_owned(), Identity::display_label)
    }

    /// Serialize this comment to its JSON contract.
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "number": self.number,
            "review_id": self.review_id,
            "session_id": self.session_id,
            "author": self.author.as_ref().map(Identity::to_json),
            "reply_to_number": self.reply_to_number,
            "anchor": self.anchor.as_ref().map(CommentAnchor::to_json),
            "body": self.body,
            "resolved": self.resolved,
            "outdated": self.outdated,
            "forge_id": self.forge_id,
        })
    }

    /// Print a newly created comment in terminal or `--json` format.
    pub fn print_created(&self, json: bool) {
        if json {
            println!("{}", json!({ "comment": self.to_json() }));
        } else {
            println!(
                "Created comment [{}] by {}",
                self.number,
                self.author_label()
            );
        }
    }

    /// Print an updated comment in terminal or `--json` format.
    pub fn print_updated(&self, json: bool) {
        if json {
            println!("{}", json!({ "comment": self.to_json() }));
        } else {
            println!("Updated comment [{}]", self.number);
        }
    }
}

impl From<&CommentDetail> for Comment {
    fn from(comment: &CommentDetail) -> Self {
        Self {
            id: comment.id.clone(),
            number: comment.number,
            review_id: comment.review_id.clone(),
            session_id: comment.session_id.clone(),
            author: comment.author.as_option().map(Identity::from),
            reply_to_number: comment.reply_to_number,
            anchor: comment.anchor.as_option().map(CommentAnchor::from),
            body: comment.body.clone(),
            resolved: comment.resolved,
            outdated: comment.outdated,
            forge_id: comment.forge_id.clone(),
        }
    }
}

impl TryFrom<CreateCommentResponse> for Comment {
    type Error = anyhow::Error;

    fn try_from(response: CreateCommentResponse) -> anyhow::Result<Self> {
        let comment = response
            .comment
            .as_option()
            .context("CreateComment response missing comment")?;
        Ok(Self::from(comment))
    }
}

impl TryFrom<UpdateCommentResponse> for Comment {
    type Error = anyhow::Error;

    fn try_from(response: UpdateCommentResponse) -> anyhow::Result<Self> {
        let comment = response
            .comment
            .as_option()
            .context("UpdateComment response missing comment")?;
        Ok(Self::from(comment))
    }
}

/// Command output for `lgtm comment delete`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeleteCommentCommandOutput {
    pub deleted_number: u64,
}

impl DeleteCommentCommandOutput {
    // Public ------------------------------------------------------------------

    /// Print the deleted comment number in terminal or `--json` format.
    pub fn print(&self, json: bool) {
        if json {
            println!("{}", json!({ "deleted_number": self.deleted_number }));
        } else {
            println!("Deleted comment [{}]", self.deleted_number);
        }
    }
}

impl From<DeleteCommentResponse> for DeleteCommentCommandOutput {
    fn from(response: DeleteCommentResponse) -> Self {
        Self {
            deleted_number: response.deleted_number,
        }
    }
}
