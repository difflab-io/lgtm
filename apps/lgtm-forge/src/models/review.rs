//! Domain model for local and forge-backed code reviews.

use std::collections::BTreeMap;

use uuid::Uuid;

use crate::{
    database,
    error::{Error, Result},
    proto::lgtm::v1::{
        ReviewGroupBy as ProtoReviewGroupBy, ReviewOutcome as ProtoReviewOutcome,
        ReviewStatus as ProtoReviewStatus, ReviewSummary,
    },
    utils::normalize_repository_url,
};

/// Discriminated review context enforcing `Local` vs. `ForgeBacked` invariants.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReviewContext {
    /// Local worktree or branch review backed only by `~/.lgtm/snapshots/$repoSlug`.
    Local { git_dir: String, branch: String },
    /// Remote pull/merge request review backed by a validated `repository_url` and `pr_number`.
    ForgeBacked {
        git_dir: String,
        branch: String,
        repository_url: String,
        pr_number: u64,
        forge_id: Option<String>,
    },
}

impl ReviewContext {
    /// Construct and validate a `Local` review context.
    pub fn new_local(git_dir: &str, branch: &str) -> Result<Self> {
        let git_dir = git_dir.trim();
        let branch = branch.trim();
        if git_dir.is_empty() || branch.is_empty() {
            return Err(Error::Invalid(
                "local review context requires non-empty git_dir and branch".to_owned(),
            ));
        }
        Ok(Self::Local {
            git_dir: git_dir.to_owned(),
            branch: branch.to_owned(),
        })
    }

    /// Construct and validate a `ForgeBacked` review context.
    pub fn new_forge_backed(
        git_dir: &str,
        branch: &str,
        repository_url: &str,
        pr_number: u64,
        forge_id: Option<String>,
    ) -> Result<Self> {
        let git_dir = git_dir.trim();
        let branch = branch.trim();
        if git_dir.is_empty() || branch.is_empty() {
            return Err(Error::Invalid(
                "forge-backed review context requires non-empty git_dir and branch".to_owned(),
            ));
        }
        if pr_number == 0 {
            return Err(Error::Invalid(
                "forge-backed review pr_number must be >= 1".to_owned(),
            ));
        }
        let repository_url = normalize_repository_url(repository_url)?;
        let forge_id = forge_id
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        Ok(Self::ForgeBacked {
            git_dir: git_dir.to_owned(),
            branch: branch.to_owned(),
            repository_url,
            pr_number,
            forge_id,
        })
    }

    // Public ------------------------------------------------------------------

    /// Return the local `git_dir` associated with this review.
    pub fn git_dir(&self) -> &str {
        match self {
            Self::Local { git_dir, .. } | Self::ForgeBacked { git_dir, .. } => git_dir,
        }
    }

    /// Return the branch name associated with this review.
    pub fn branch(&self) -> &str {
        match self {
            Self::Local { branch, .. } | Self::ForgeBacked { branch, .. } => branch,
        }
    }

    /// Return true when this review is `Local`.
    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local { .. })
    }

    /// Require that this review is `ForgeBacked` before executing a remote forge operation, returning `(repository_url, pr_number, forge_id)`.
    pub fn require_forge_backed(&self) -> Result<(&str, u64, Option<&str>)> {
        match self {
            Self::Local { .. } => Err(Error::Invalid(
                "remote forge operations are not permitted on a Local review context".to_owned(),
            )),
            Self::ForgeBacked {
                repository_url,
                pr_number,
                forge_id,
                ..
            } => Ok((repository_url.as_str(), *pr_number, forge_id.as_deref())),
        }
    }
}

/// Lifecycle status of a review.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewStatus {
    Open,
    Closed,
    Merged,
}

/// Disposition recorded when closing/submitting a review session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewOutcome {
    Comment,
    Reject,
    Approve,
    Request,
}

/// Presentation grouping mode for `ShowReview`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReviewGroupBy {
    #[default]
    File,
    Session,
}

/// Application review entity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Review {
    pub id: Uuid,
    pub number: u64,
    pub name: String,
    pub context: ReviewContext,
    pub status: ReviewStatus,
    pub base_sha: String,
    pub head_sha: String,
    pub reviewed_files: BTreeMap<String, String>,
}

impl Review {
    /// Convert a persisted database review row into a domain `Review`.
    pub fn from_db(model: database::Review) -> Result<Self> {
        let number = u64::try_from(model.number)
            .map_err(|_| Error::Invalid("review.number must be positive".to_owned()))?;
        let context = match model.kind {
            database::ReviewKind::Local => ReviewContext::new_local(&model.git_dir, &model.branch)?,
            database::ReviewKind::ForgeBacked => {
                let repository_url = model.repository_url.ok_or_else(|| {
                    Error::Invalid("ForgeBacked review is missing repository_url".to_owned())
                })?;
                let pr_number = model
                    .pr_number
                    .and_then(|n| u64::try_from(n).ok())
                    .ok_or_else(|| {
                        Error::Invalid("ForgeBacked review is missing valid pr_number".to_owned())
                    })?;
                ReviewContext::new_forge_backed(
                    &model.git_dir,
                    &model.branch,
                    &repository_url,
                    pr_number,
                    model.forge_id,
                )?
            }
        };
        let status = match model.status {
            database::ReviewStatus::Open => ReviewStatus::Open,
            database::ReviewStatus::Closed => ReviewStatus::Closed,
            database::ReviewStatus::Merged => ReviewStatus::Merged,
        };
        let reviewed_files =
            serde_json::from_value::<BTreeMap<String, String>>(model.reviewed_files)
                .unwrap_or_default();

        Ok(Self {
            id: model.id,
            number,
            name: model.name,
            context,
            status,
            base_sha: model.base_sha,
            head_sha: model.head_sha,
            reviewed_files,
        })
    }

    // Public ------------------------------------------------------------------

    /// Format a greeting message for the `HelloReview` demonstration endpoint.
    pub fn format_hello(greeting: &str, name: &str) -> String {
        let name = name.trim();
        let name = if name.is_empty() { "World" } else { name };
        format!("{greeting}, {name}!")
    }
}

impl From<ReviewStatus> for database::ReviewStatus {
    fn from(status: ReviewStatus) -> Self {
        match status {
            ReviewStatus::Open => Self::Open,
            ReviewStatus::Closed => Self::Closed,
            ReviewStatus::Merged => Self::Merged,
        }
    }
}

impl From<ReviewStatus> for ProtoReviewStatus {
    fn from(status: ReviewStatus) -> Self {
        match status {
            ReviewStatus::Open => Self::REVIEW_STATUS_OPEN,
            ReviewStatus::Closed => Self::REVIEW_STATUS_CLOSED,
            ReviewStatus::Merged => Self::REVIEW_STATUS_MERGED,
        }
    }
}

impl From<database::ReviewOutcome> for ReviewOutcome {
    fn from(outcome: database::ReviewOutcome) -> Self {
        match outcome {
            database::ReviewOutcome::Comment => Self::Comment,
            database::ReviewOutcome::Reject => Self::Reject,
            database::ReviewOutcome::Approve => Self::Approve,
            database::ReviewOutcome::Request => Self::Request,
        }
    }
}

impl From<ReviewOutcome> for database::ReviewOutcome {
    fn from(outcome: ReviewOutcome) -> Self {
        match outcome {
            ReviewOutcome::Comment => Self::Comment,
            ReviewOutcome::Reject => Self::Reject,
            ReviewOutcome::Approve => Self::Approve,
            ReviewOutcome::Request => Self::Request,
        }
    }
}

impl From<ReviewOutcome> for ProtoReviewOutcome {
    fn from(outcome: ReviewOutcome) -> Self {
        match outcome {
            ReviewOutcome::Comment => Self::REVIEW_OUTCOME_COMMENT,
            ReviewOutcome::Reject => Self::REVIEW_OUTCOME_REJECT,
            ReviewOutcome::Approve => Self::REVIEW_OUTCOME_APPROVE,
            ReviewOutcome::Request => Self::REVIEW_OUTCOME_REQUEST,
        }
    }
}

impl TryFrom<ProtoReviewOutcome> for ReviewOutcome {
    type Error = Error;

    fn try_from(value: ProtoReviewOutcome) -> Result<Self> {
        match value {
            ProtoReviewOutcome::REVIEW_OUTCOME_COMMENT
            | ProtoReviewOutcome::REVIEW_OUTCOME_UNSPECIFIED => Ok(Self::Comment),
            ProtoReviewOutcome::REVIEW_OUTCOME_REJECT => Ok(Self::Reject),
            ProtoReviewOutcome::REVIEW_OUTCOME_APPROVE => Ok(Self::Approve),
            ProtoReviewOutcome::REVIEW_OUTCOME_REQUEST => Ok(Self::Request),
        }
    }
}

impl From<ProtoReviewGroupBy> for ReviewGroupBy {
    fn from(value: ProtoReviewGroupBy) -> Self {
        match value {
            ProtoReviewGroupBy::REVIEW_GROUP_BY_SESSION => Self::Session,
            ProtoReviewGroupBy::REVIEW_GROUP_BY_FILE
            | ProtoReviewGroupBy::REVIEW_GROUP_BY_UNSPECIFIED => Self::File,
        }
    }
}

impl From<Review> for ReviewSummary {
    fn from(review: Review) -> Self {
        let (local, git_dir, branch, repository_url, pr_number, forge_id) = match review.context {
            ReviewContext::Local { git_dir, branch } => (true, git_dir, branch, None, None, None),
            ReviewContext::ForgeBacked {
                git_dir,
                branch,
                repository_url,
                pr_number,
                forge_id,
            } => (
                false,
                git_dir,
                branch,
                Some(repository_url),
                Some(pr_number),
                forge_id,
            ),
        };
        Self {
            id: review.id.to_string(),
            number: review.number,
            name: review.name,
            local,
            git_dir,
            branch,
            repository_url,
            pr_number,
            forge_id,
            status: ProtoReviewStatus::from(review.status).into(),
            base_sha: review.base_sha,
            head_sha: review.head_sha,
            reviewed_files: review.reviewed_files.into_iter().collect(),
            ..Default::default()
        }
    }
}

// tests -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_reject_remote_operations_for_local_review_context() {
        let local = ReviewContext::new_local("/tmp/repo/.git", "feature/test").unwrap();
        assert!(local.is_local());
        assert!(local.require_forge_backed().is_err());

        let forge = ReviewContext::new_forge_backed(
            "/tmp/repo/.git",
            "feature/test",
            "https://github.com/difflab-io/lgtm",
            42,
            Some("PR_kwDO123".to_owned()),
        )
        .unwrap();
        assert!(!forge.is_local());
        assert_eq!(
            forge.require_forge_backed().unwrap(),
            ("https://github.com/difflab-io/lgtm", 42, Some("PR_kwDO123"))
        );
    }
}
