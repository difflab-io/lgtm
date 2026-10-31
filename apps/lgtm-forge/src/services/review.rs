//! Core `ReviewService` orchestrating local snapshot reviews and remote forge synchronization.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
};

use uuid::Uuid;

use crate::{
    database::{
        CommentAnchorShift, Database, NewComment, NewLocalReview, ReviewFilter, ReviewKind,
        ReviewStatus as DbReviewStatus, SessionState as DbSessionState, UpdateComment,
        UpsertForgeComment, UpsertForgeReview, UpsertForgeSession,
    },
    error::{Error, Result},
    extensions::gitoxide,
    forge::{
        ForgeCommentUpdate, ForgeDraftCommentInput, ForgePullRequestBundle, GitHubForge,
        RemoteForge,
    },
    models::{
        Comment, CommentAnchor, DiffHunk, Identity, Review, ReviewGroupBy, ReviewOutcome, Session,
        comment::format_body_with_disclosure,
    },
    snapshots::SnapshotStore,
    utils::parse_agent_descriptor,
};

/// Result of a `Commit` operation (`local == true` shadow commit or `local == false` working tree commit).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitOutcome {
    pub commit_sha: String,
    pub branch: String,
    pub author: Identity,
    pub local: bool,
}

/// Result of a `Push` operation to the remote repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PushOutcome {
    pub branch: String,
    pub head_sha: String,
    pub repository_url: String,
    pub local: bool,
}

/// Aggregated payload returned by `ShowReview`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewDetail {
    pub review: Review,
    pub sessions: Vec<Session>,
    pub comments: Vec<Comment>,
    pub diff_hunks: Vec<DiffHunk>,
}

/// Parameters for creating a new local or remote draft comment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateCommentInput {
    pub git_dir: String,
    pub review_selector: Option<String>,
    pub body: String,
    pub file_path: Option<String>,
    pub start_line: Option<u32>,
    pub line: Option<u32>,
    pub reply_to: Option<u64>,
    pub local: bool,
    pub as_agent: Option<String>,
}

/// Parameters for updating an existing comment's body, anchor, or thread resolution state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateCommentInput {
    pub git_dir: String,
    pub comment_number: u64,
    pub body: Option<String>,
    pub file_path: Option<String>,
    pub start_line: Option<u32>,
    pub line: Option<u32>,
    pub resolved: Option<bool>,
    pub local: bool,
    pub as_agent: Option<String>,
}

/// Result of marking or unmarking a file as viewed in a review.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileReviewedOutcome {
    pub file_path: String,
    pub viewed: bool,
    pub commit_sha: String,
}

/// Application service coordinating `lgtm-db`, `SnapshotStore`, `extensions::gitoxide`, and `RemoteForge`.
#[derive(Clone)]
pub struct ReviewService {
    database: Database,
    snapshots: SnapshotStore,
    forge: Arc<dyn RemoteForge>,
}

impl ReviewService {
    /// Construct a `ReviewService` with default `SnapshotStore` and `GitHubForge` dependencies.
    pub fn new(database: Database) -> Self {
        Self::with_dependencies(
            database,
            SnapshotStore::default(),
            Arc::new(GitHubForge::new()),
        )
    }

    /// Construct a `ReviewService` with explicit `SnapshotStore` and `RemoteForge` dependencies.
    pub fn with_dependencies(
        database: Database,
        snapshots: SnapshotStore,
        forge: Arc<dyn RemoteForge>,
    ) -> Self {
        Self {
            database,
            snapshots,
            forge,
        }
    }

    // Public ------------------------------------------------------------------

    // Git & Snapshots

    /// Stage worktree paths into `SnapshotStore` (`local == true`) or the working checkout (`local == false`).
    pub async fn add_paths(
        &self,
        git_dir: &str,
        paths: &[String],
        local: bool,
    ) -> Result<Vec<String>> {
        tracing::info!(git_dir, local, path_count = paths.len(), "staging paths");
        if local {
            self.snapshots.stage_paths(git_dir, paths)
        } else {
            gitoxide::stage_paths(git_dir, paths)
        }
    }

    /// Create a shadow commit (`local == true`) or working checkout commit mirrored to `SnapshotStore` (`local == false`).
    pub async fn commit_changes(
        &self,
        git_dir: &str,
        message: Option<&str>,
        as_agent: Option<&str>,
        local: bool,
    ) -> Result<CommitOutcome> {
        let ctx = gitoxide::get_repo_context(git_dir)?;
        let author = self.resolve_actor(&ctx, as_agent).await?;
        let (commit_sha, branch) = if local {
            self.snapshots.commit(&ctx.git_dir, message, &author)?
        } else {
            let (sha, branch) = gitoxide::create_commit(&ctx.git_dir, message, &author)?;
            self.snapshots.mirror_checkout_commits(&ctx.git_dir, &sha)?;
            (sha, branch)
        };
        tracing::info!(commit_sha, branch, local, "created commit");
        Ok(CommitOutcome {
            commit_sha,
            branch,
            author,
            local,
        })
    }

    /// Push committed history on the active branch to the remote repository (`local == false` only).
    pub async fn push_changes(&self, git_dir: &str, local: bool) -> Result<PushOutcome> {
        if local {
            return Err(Error::Invalid(
                "remote push is not permitted when local snapshot mode (--local) is active"
                    .to_owned(),
            ));
        }
        let ctx = gitoxide::get_repo_context(git_dir)?;
        let (head_sha, repository_url) = gitoxide::push_branch(&ctx.git_dir, &ctx.branch)?;
        Ok(PushOutcome {
            branch: ctx.branch,
            head_sha,
            repository_url,
            local: false,
        })
    }

    // Reviews

    /// List reviews in `git_dir` filtered by `local` context and `all` status flag.
    pub async fn get_reviews(&self, git_dir: &str, local: bool, all: bool) -> Result<Vec<Review>> {
        let ctx = gitoxide::get_repo_context(git_dir)?;
        let filter = ReviewFilter {
            kind: Some(if local {
                ReviewKind::Local
            } else {
                ReviewKind::ForgeBacked
            }),
            status: if all {
                None
            } else {
                Some(DbReviewStatus::Open)
            },
            branch: None,
        };
        let rows = self
            .database
            .reviews
            .get_reviews(&ctx.git_dir, filter)
            .await?;
        rows.into_iter().map(Review::from_db).collect()
    }

    /// Create a new local snapshot review (`local == true`) or synchronize a remote pull request review (`local == false`).
    pub async fn create_review(&self, git_dir: &str, local: bool) -> Result<(Review, Session)> {
        let ctx = gitoxide::get_repo_context(git_dir)?;
        let actor = self.resolve_actor(&ctx, None).await?;

        if local {
            let (base_sha, head_sha) = self.snapshots.ensure_snapshot(&ctx.git_dir)?;
            let tx = self.database.begin().await?;
            let db_review = tx
                .reviews
                .create_local_review(NewLocalReview {
                    git_dir: ctx.git_dir.clone(),
                    branch: ctx.branch.clone(),
                    base_sha,
                    head_sha,
                })
                .await?;
            let db_session = tx
                .sessions
                .get_or_create_draft_session(db_review.id, actor.id, None)
                .await?;
            tx.commit().await?;
            return Ok((
                Review::from_db(db_review)?,
                Session::from_db(db_session, actor)?,
            ));
        }

        let repository_url = ctx.repository_url.clone().ok_or_else(|| {
            Error::Invalid(
                "working repository has no 'origin' remote URL configured for remote review"
                    .to_owned(),
            )
        })?;
        let bundle = self
            .forge
            .get_pull_request_bundle(&repository_url, &ctx.branch)
            .await?;
        let _ = self
            .snapshots
            .mirror_checkout_commits(&ctx.git_dir, &bundle.pull_request.head_sha);
        let (review, session, _) = self
            .import_pr_bundle(&ctx.git_dir, &repository_url, &actor, bundle)
            .await?;
        Ok((review, session))
    }

    /// Refresh an existing review, remapping surviving line anchors, marking overwritten anchors outdated, pruning modified `reviewed_files`, and syncing with `RemoteForge` when `local == false`.
    pub async fn refresh_review(
        &self,
        git_dir: &str,
        selector: Option<&str>,
        local: bool,
    ) -> Result<(Review, u32)> {
        let ctx = gitoxide::get_repo_context(git_dir)?;
        let actor = self.resolve_actor(&ctx, None).await?;

        if local {
            let existing = self
                .resolve_review(&ctx.git_dir, &ctx.branch, selector, true)
                .await?;
            let (base_sha, new_head_sha) = self.snapshots.ensure_snapshot(&ctx.git_dir)?;
            let comments = self
                .load_review_comments(existing.id, Some(actor.id))
                .await?;
            let shifts = self.snapshots.recompute_anchors(
                &ctx.git_dir,
                &existing.head_sha,
                &new_head_sha,
                &comments,
            )?;
            let updated_anchors = u32::try_from(shifts.len()).unwrap_or(u32::MAX);
            let next_reviewed_files =
                self.prune_modified_reviewed_files(&ctx.git_dir, &existing, &new_head_sha)?;

            let tx = self.database.begin().await?;
            tx.reviews
                .update_review_commits(existing.id, &base_sha, &new_head_sha)
                .await?;
            let updated_review = tx
                .reviews
                .update_reviewed_files(existing.id, next_reviewed_files)
                .await?;
            tx.comments.update_comment_anchors(&shifts).await?;
            tx.commit().await?;

            return Ok((Review::from_db(updated_review)?, updated_anchors));
        }

        let maybe_existing = self
            .find_existing_review(&ctx.git_dir, &ctx.branch, selector, false)
            .await?;
        if let Some(existing_review) = &maybe_existing {
            self.flush_unsynced_drafts(existing_review, &actor).await?;
        }

        let repository_url = if let Some(existing) = &maybe_existing {
            let (url, _, _) = existing.context.require_forge_backed()?;
            url.to_owned()
        } else {
            ctx.repository_url.clone().ok_or_else(|| {
                Error::Invalid(
                    "working repository has no 'origin' remote URL configured".to_owned(),
                )
            })?
        };
        let target_selector = selector.unwrap_or(&ctx.branch);
        let bundle = self
            .forge
            .get_pull_request_bundle(&repository_url, target_selector)
            .await?;
        let _ = self
            .snapshots
            .mirror_checkout_commits(&ctx.git_dir, &bundle.pull_request.head_sha);

        let (review, _, updated_anchors) = self
            .import_pr_bundle(&ctx.git_dir, &repository_url, &actor, bundle)
            .await?;
        Ok((review, updated_anchors))
    }

    /// Load a review along with its sessions, visible comments (optionally filtered to unresolved threads), and inline diff hunks.
    pub async fn get_review_detail(
        &self,
        git_dir: &str,
        selector: Option<&str>,
        local: bool,
        group_by: ReviewGroupBy,
        unresolved_only: bool,
    ) -> Result<ReviewDetail> {
        let ctx = gitoxide::get_repo_context(git_dir)?;
        let actor = self.resolve_actor(&ctx, None).await?;
        let review = self
            .resolve_review(&ctx.git_dir, &ctx.branch, selector, local)
            .await?;
        let sessions = self.load_review_sessions(review.id).await?;
        let all_comments = self.load_review_comments(review.id, Some(actor.id)).await?;

        let filtered_comments = if unresolved_only {
            let unresolved_roots: HashSet<u64> = all_comments
                .iter()
                .filter(|c| c.reply_to_number.is_none() && !c.resolved)
                .map(|c| c.number)
                .collect();
            all_comments
                .into_iter()
                .filter(|c| match c.reply_to_number {
                    None => !c.resolved,
                    Some(root_num) => unresolved_roots.contains(&root_num),
                })
                .collect()
        } else {
            all_comments
        };

        let ordered_comments = order_comments_for_display(filtered_comments, group_by);
        let anchors: Vec<CommentAnchor> = ordered_comments
            .iter()
            .filter(|c| c.reply_to_number.is_none())
            .filter_map(|c| c.anchor.clone())
            .collect();
        let diff_hunks = self
            .snapshots
            .get_inline_diff_hunks(&ctx.git_dir, &review.base_sha, &review.head_sha, &anchors)
            .unwrap_or_default();

        Ok(ReviewDetail {
            review,
            sessions,
            comments: ordered_comments,
            diff_hunks,
        })
    }

    /// Close/publish the caller's open draft review session locally (`local == true`) or submit it to the remote forge (`local == false`).
    pub async fn close_review(
        &self,
        git_dir: &str,
        selector: Option<&str>,
        local: bool,
        outcome: ReviewOutcome,
        body: Option<String>,
        as_agent: Option<&str>,
    ) -> Result<(Review, Session)> {
        let ctx = gitoxide::get_repo_context(git_dir)?;
        let actor = self.resolve_actor(&ctx, as_agent).await?;
        let review = self
            .resolve_review(&ctx.git_dir, &ctx.branch, selector, local)
            .await?;

        if review.context.is_local() {
            let tx = self.database.begin().await?;
            let draft = tx
                .sessions
                .get_or_create_draft_session(review.id, actor.id, body.clone())
                .await?;
            let published = tx
                .sessions
                .publish_session(draft.id, outcome.into(), body, None)
                .await?;
            tx.commit().await?;
            return Ok((review, Session::from_db(published, actor)?));
        }

        self.flush_unsynced_drafts(&review, &actor).await?;
        let (repository_url, pr_number, _) = review.context.require_forge_backed()?;
        let db_draft = self
            .database
            .sessions
            .get_or_create_draft_session(review.id, actor.id, body.clone())
            .await?;
        let draft_domain = Session::from_db(db_draft.clone(), actor.clone())?;
        let export_body = draft_domain.format_export_body(body.as_deref());

        let receipt = self
            .forge
            .submit_pending_review(
                repository_url,
                pr_number,
                draft_domain.forge_id.as_deref(),
                outcome,
                export_body.as_deref(),
            )
            .await?;

        let published = self
            .database
            .sessions
            .publish_session(
                db_draft.id,
                outcome.into(),
                body,
                Some(receipt.review_forge_id),
            )
            .await?;
        Ok((review, Session::from_db(published, actor)?))
    }

    // Comments & Viewed Files

    /// Create a general, inline, range, or reply comment, persisting it locally first and syncing to `RemoteForge` when `local == false`.
    pub async fn create_comment(&self, input: CreateCommentInput) -> Result<Comment> {
        let ctx = gitoxide::get_repo_context(&input.git_dir)?;
        let actor = self.resolve_actor(&ctx, input.as_agent.as_deref()).await?;
        let review = self
            .resolve_review(
                &ctx.git_dir,
                &ctx.branch,
                input.review_selector.as_deref(),
                input.local,
            )
            .await?;

        let mut reply_to_id = None;
        let mut parent_forge_id = None;
        if let Some(reply_num) = input.reply_to {
            if input.file_path.is_some() || input.line.is_some() || input.start_line.is_some() {
                return Err(Error::Invalid(
                    "reply comments inherit their thread's diff anchor and must not specify file_path or line".to_owned(),
                ));
            }
            let parent = self
                .database
                .comments
                .get_comment_by_number(i64::try_from(reply_num).unwrap_or(-1))
                .await?
                .ok_or_else(|| Error::NotFound(format!("comment C{reply_num} not found")))?;
            if parent.review_id != review.id {
                return Err(Error::Invalid(format!(
                    "comment C{reply_num} does not belong to review '{}'",
                    review.name
                )));
            }
            reply_to_id = Some(parent.id);
            parent_forge_id = parent.forge_id;
        } else if let Some(file_path) = input.file_path.as_deref() {
            let line = input.line.ok_or_else(|| {
                Error::Invalid("file-anchored comment requires --line".to_owned())
            })?;
            let _ = CommentAnchor::new(file_path, input.start_line, line, &review.head_sha)?;
            self.snapshots.validate_diff_line(
                &ctx.git_dir,
                &review.base_sha,
                &review.head_sha,
                file_path,
                input.start_line,
                line,
            )?;
        } else if input.line.is_some() || input.start_line.is_some() {
            return Err(Error::Invalid(
                "line-anchored comment requires --file".to_owned(),
            ));
        }

        let commit_sha = if input.file_path.is_some() {
            Some(review.head_sha.clone())
        } else {
            None
        };

        let tx = self.database.begin().await?;
        let draft_session = tx
            .sessions
            .get_or_create_draft_session(review.id, actor.id, None)
            .await?;
        let created = tx
            .comments
            .create_comment(NewComment {
                review_id: review.id,
                session_id: draft_session.id,
                author_id: actor.id,
                reply_to_id,
                file_path: input.file_path.clone(),
                start_line: input.start_line.and_then(|v| i32::try_from(v).ok()),
                line: input.line.and_then(|v| i32::try_from(v).ok()),
                commit_sha,
                body: input.body.clone(),
            })
            .await?;
        tx.commit().await?;

        let mut domain_comment = Comment::from_db(created.clone(), actor.clone(), input.reply_to)?;

        if let Ok((repository_url, pr_number, _)) = review.context.require_forge_backed() {
            let export_body = domain_comment.format_export_body();
            let draft_input = ForgeDraftCommentInput {
                body: export_body,
                file_path: input.file_path,
                start_line: input.start_line,
                line: input.line,
                commit_sha: review.head_sha.clone(),
                reply_to_forge_id: parent_forge_id,
            };
            if let Ok(receipt) = self
                .forge
                .create_remote_draft_comment(
                    repository_url,
                    pr_number,
                    draft_session.forge_id.as_deref(),
                    &draft_input,
                )
                .await
            {
                let tx = self.database.begin().await?;
                if draft_session.forge_id.is_none() {
                    tx.sessions
                        .upsert_forge_session(UpsertForgeSession {
                            review_id: review.id,
                            author_id: actor.id,
                            forge_id: receipt.pending_review_forge_id,
                            state: DbSessionState::Draft,
                            outcome: None,
                            body: draft_session.body,
                            published_at: None,
                        })
                        .await?;
                }
                let updated = tx
                    .comments
                    .update_comment(
                        created.id,
                        UpdateComment {
                            forge_id: Some(receipt.comment_forge_id),
                            ..Default::default()
                        },
                    )
                    .await?;
                tx.commit().await?;
                domain_comment = Comment::from_db(updated, actor, input.reply_to)?;
            }
        }

        Ok(domain_comment)
    }

    /// Update an existing comment's body, anchor, or thread resolution status locally and on `RemoteForge` when `local == false`.
    pub async fn update_comment(&self, input: UpdateCommentInput) -> Result<Comment> {
        let ctx = gitoxide::get_repo_context(&input.git_dir)?;
        let actor = self.resolve_actor(&ctx, input.as_agent.as_deref()).await?;
        let (existing, review) = self
            .load_comment_and_review(input.comment_number, input.local)
            .await?;

        if input.line.is_some() || input.start_line.is_some() {
            let file_path = input
                .file_path
                .as_deref()
                .or(existing.file_path.as_deref())
                .ok_or_else(|| {
                    Error::Invalid("updating --line requires a file-anchored comment".to_owned())
                })?;
            let new_line = input
                .line
                .or_else(|| existing.line.and_then(|value| u32::try_from(value).ok()))
                .ok_or_else(|| {
                    Error::Invalid("updating anchor requires a valid target line".to_owned())
                })?;
            let start_line = input.start_line.or_else(|| {
                existing
                    .start_line
                    .and_then(|value| u32::try_from(value).ok())
            });
            self.snapshots.validate_diff_line(
                &ctx.git_dir,
                &review.base_sha,
                &review.head_sha,
                file_path,
                start_line,
                new_line,
            )?;
            self.database
                .comments
                .update_comment_anchors(&[CommentAnchorShift {
                    comment_id: existing.id,
                    start_line: start_line.and_then(|v| i32::try_from(v).ok()),
                    line: i32::try_from(new_line).ok(),
                    commit_sha: Some(review.head_sha.clone()),
                    outdated: false,
                }])
                .await?;
        }

        let updated = self
            .database
            .comments
            .update_comment(
                existing.id,
                UpdateComment {
                    body: input.body.clone(),
                    resolved: input.resolved,
                    ..Default::default()
                },
            )
            .await?;

        let reply_to_number = self.resolve_reply_to_number(updated.reply_to_id).await?;
        let author = self.load_identity(updated.author_id).await?;
        let domain_comment = Comment::from_db(updated, author, reply_to_number)?;

        if let Ok((repository_url, _, _)) = review.context.require_forge_backed()
            && let Some(forge_id) = &domain_comment.forge_id
        {
            let export_body = input
                .body
                .as_deref()
                .map(|raw| format_body_with_disclosure(raw, &actor));
            self.forge
                .update_remote_comment(
                    repository_url,
                    forge_id,
                    &ForgeCommentUpdate {
                        body: export_body,
                        file_path: domain_comment.anchor.as_ref().map(|a| a.file_path.clone()),
                        start_line: domain_comment.anchor.as_ref().and_then(|a| a.start_line),
                        line: domain_comment.anchor.as_ref().map(|a| a.line),
                        commit_sha: domain_comment.anchor.as_ref().map(|a| a.commit_sha.clone()),
                        resolved: input.resolved,
                    },
                )
                .await?;
        }

        Ok(domain_comment)
    }

    /// Delete a comment (cascading to thread replies when deleting a root comment) locally and on `RemoteForge` when `local == false`.
    pub async fn delete_comment(
        &self,
        git_dir: &str,
        comment_number: u64,
        local: bool,
        as_agent: Option<&str>,
    ) -> Result<u64> {
        let ctx = gitoxide::get_repo_context(git_dir)?;
        let _actor = self.resolve_actor(&ctx, as_agent).await?;
        let (existing, review) = self.load_comment_and_review(comment_number, local).await?;

        if let Ok((repository_url, _, _)) = review.context.require_forge_backed()
            && let Some(forge_id) = &existing.forge_id
        {
            self.forge
                .delete_remote_comment(repository_url, forge_id)
                .await?;
        }

        self.database.comments.delete_comment(existing.id).await?;
        Ok(comment_number)
    }

    /// Mark or unmark `file_path` as viewed at `review.head_sha`, syncing with `RemoteForge` when `ForgeBacked`.
    pub async fn set_file_reviewed(
        &self,
        git_dir: &str,
        review_selector: Option<&str>,
        file_path: &str,
        viewed: bool,
        local: bool,
    ) -> Result<FileReviewedOutcome> {
        let ctx = gitoxide::get_repo_context(git_dir)?;
        let review = self
            .resolve_review(&ctx.git_dir, &ctx.branch, review_selector, local)
            .await?;

        let updated = if viewed {
            self.database
                .reviews
                .mark_file_reviewed(review.id, file_path, &review.head_sha)
                .await?
        } else {
            self.database
                .reviews
                .unmark_file_reviewed(review.id, file_path)
                .await?
        };
        let updated_domain = Review::from_db(updated)?;

        if let Ok((repository_url, pr_number, pr_forge_id)) =
            updated_domain.context.require_forge_backed()
        {
            self.forge
                .set_remote_file_viewed(repository_url, pr_forge_id, pr_number, file_path, viewed)
                .await?;
        }

        Ok(FileReviewedOutcome {
            file_path: file_path.trim().to_owned(),
            viewed,
            commit_sha: updated_domain.head_sha,
        })
    }

    /// Format a greeting message for the `HelloReview` demonstration endpoint.
    pub fn get_hello(&self, name: &str) -> String {
        Review::format_hello(&self.database.get_hello(), name)
    }

    // Private -----------------------------------------------------------------

    async fn resolve_actor(
        &self,
        ctx: &gitoxide::RepoContext,
        as_agent: Option<&str>,
    ) -> Result<Identity> {
        if let Some(raw_agent) = as_agent.filter(|s| !s.trim().is_empty()) {
            let (name, provider, model) = parse_agent_descriptor(raw_agent)?;
            let row = self
                .database
                .identities
                .upsert_agent(&name, &provider, &model)
                .await?;
            return Identity::from_db(row);
        }
        let row = self
            .database
            .identities
            .upsert_human(&ctx.user_name, &ctx.user_email, None)
            .await?;
        Identity::from_db(row)
    }

    async fn find_existing_review(
        &self,
        git_dir: &str,
        branch: &str,
        selector: Option<&str>,
        local: bool,
    ) -> Result<Option<Review>> {
        if let Some(sel) = selector.map(str::trim).filter(|s| !s.is_empty()) {
            if let Some(row) = self
                .database
                .reviews
                .get_review_by_selector(git_dir, sel)
                .await?
            {
                let review = Review::from_db(row)?;
                if review.context.is_local() == local {
                    return Ok(Some(review));
                }
            }
            return Ok(None);
        }

        let filter = ReviewFilter {
            kind: Some(if local {
                ReviewKind::Local
            } else {
                ReviewKind::ForgeBacked
            }),
            status: None,
            branch: Some(branch.to_owned()),
        };
        let rows = self.database.reviews.get_reviews(git_dir, filter).await?;
        rows.into_iter().next().map(Review::from_db).transpose()
    }

    async fn resolve_review(
        &self,
        git_dir: &str,
        branch: &str,
        selector: Option<&str>,
        local: bool,
    ) -> Result<Review> {
        self.find_existing_review(git_dir, branch, selector, local)
            .await?
            .ok_or_else(|| {
                Error::NotFound(format!(
                    "no {} review found for selector '{}' on branch '{branch}'",
                    if local { "local" } else { "remote" },
                    selector.unwrap_or(branch)
                ))
            })
    }

    async fn load_comment_and_review(
        &self,
        comment_number: u64,
        local: bool,
    ) -> Result<(crate::database::Comment, Review)> {
        let existing = self
            .database
            .comments
            .get_comment_by_number(i64::try_from(comment_number).unwrap_or(-1))
            .await?
            .ok_or_else(|| Error::NotFound(format!("comment C{comment_number} not found")))?;
        let db_review = self
            .database
            .reviews
            .get_review(existing.review_id)
            .await?
            .ok_or_else(|| Error::NotFound("owning review not found".to_owned()))?;
        let review = Review::from_db(db_review)?;
        if review.context.is_local() != local {
            return Err(Error::Invalid(format!(
                "comment C{comment_number} belongs to a {} review; pass matching --local flag",
                if review.context.is_local() {
                    "local"
                } else {
                    "remote"
                }
            )));
        }
        Ok((existing, review))
    }

    async fn flush_unsynced_drafts(&self, review: &Review, actor: &Identity) -> Result<()> {
        let (repository_url, pr_number, _) = review.context.require_forge_backed()?;
        let unsynced = self
            .database
            .comments
            .get_unsynced_comments_by_review(review.id, actor.id)
            .await?;
        if unsynced.is_empty() {
            return Ok(());
        }

        let mut draft_session = self
            .database
            .sessions
            .get_or_create_draft_session(review.id, actor.id, None)
            .await?;

        for row in unsynced {
            let (reply_to_number, reply_to_forge_id) = if let Some(parent_id) = row.reply_to_id {
                let parent = self
                    .database
                    .comments
                    .get_comment(parent_id)
                    .await?
                    .ok_or_else(|| {
                        Error::NotFound(format!("parent comment {parent_id} not found"))
                    })?;
                (u64::try_from(parent.number).ok(), parent.forge_id)
            } else {
                (None, None)
            };
            let domain_comment = Comment::from_db(row.clone(), actor.clone(), reply_to_number)?;
            let export_body = domain_comment.format_export_body();
            let receipt = self
                .forge
                .create_remote_draft_comment(
                    repository_url,
                    pr_number,
                    draft_session.forge_id.as_deref(),
                    &ForgeDraftCommentInput {
                        body: export_body,
                        file_path: row.file_path.clone(),
                        start_line: row.start_line.and_then(|v| u32::try_from(v).ok()),
                        line: row.line.and_then(|v| u32::try_from(v).ok()),
                        commit_sha: row
                            .commit_sha
                            .clone()
                            .unwrap_or_else(|| review.head_sha.clone()),
                        reply_to_forge_id,
                    },
                )
                .await?;

            if draft_session.forge_id.is_none() {
                draft_session = self
                    .database
                    .sessions
                    .upsert_forge_session(UpsertForgeSession {
                        review_id: review.id,
                        author_id: actor.id,
                        forge_id: receipt.pending_review_forge_id,
                        state: DbSessionState::Draft,
                        outcome: None,
                        body: draft_session.body,
                        published_at: None,
                    })
                    .await?;
            }
            self.database
                .comments
                .update_comment(
                    row.id,
                    UpdateComment {
                        forge_id: Some(receipt.comment_forge_id),
                        ..Default::default()
                    },
                )
                .await?;
        }
        Ok(())
    }

    async fn import_pr_bundle(
        &self,
        git_dir: &str,
        repository_url: &str,
        actor: &Identity,
        bundle: ForgePullRequestBundle,
    ) -> Result<(Review, Session, u32)> {
        let pr = bundle.pull_request;
        let existing_review = self
            .find_existing_review(
                git_dir,
                &pr.branch,
                Some(&format!("PR{}", pr.pr_number)),
                false,
            )
            .await?;
        let old_head_sha = existing_review.as_ref().map(|r| r.head_sha.clone());

        let tx = self.database.begin().await?;
        let db_review = tx
            .reviews
            .upsert_forge_review(UpsertForgeReview {
                git_dir: git_dir.to_owned(),
                branch: pr.branch.clone(),
                repository_url: repository_url.to_owned(),
                pr_number: i64::try_from(pr.pr_number).unwrap_or(1),
                forge_id: Some(pr.forge_id.clone()),
                base_sha: pr.base_sha.clone(),
                head_sha: pr.head_sha.clone(),
                status: pr.status.into(),
            })
            .await?;

        let mut session_id_by_forge_id: HashMap<String, Uuid> = HashMap::new();
        for remote_sess in bundle.sessions {
            let author_row = tx
                .identities
                .upsert_human(
                    &remote_sess.author_name,
                    &remote_sess.author_email,
                    Some(&remote_sess.author_login),
                )
                .await?;
            let saved_sess = tx
                .sessions
                .upsert_forge_session(UpsertForgeSession {
                    review_id: db_review.id,
                    author_id: author_row.id,
                    forge_id: remote_sess.forge_id.clone(),
                    state: remote_sess.state.into(),
                    outcome: remote_sess.outcome.map(Into::into),
                    body: remote_sess.body,
                    published_at: remote_sess.published_at,
                })
                .await?;
            session_id_by_forge_id.insert(remote_sess.forge_id, saved_sess.id);
        }

        let mut comment_id_by_forge_id: HashMap<String, Uuid> = HashMap::new();
        let (roots, replies): (Vec<_>, Vec<_>) = bundle
            .comments
            .into_iter()
            .partition(|c| c.reply_to_forge_id.is_none());

        for remote_comment in roots.into_iter().chain(replies.into_iter()) {
            let author_row = tx
                .identities
                .upsert_human(
                    &remote_comment.author_name,
                    &remote_comment.author_email,
                    Some(&remote_comment.author_login),
                )
                .await?;
            let session_id = if let Some(&id) =
                session_id_by_forge_id.get(&remote_comment.session_forge_id)
            {
                id
            } else {
                let fallback = tx
                    .sessions
                    .upsert_forge_session(UpsertForgeSession {
                        review_id: db_review.id,
                        author_id: author_row.id,
                        forge_id: remote_comment.session_forge_id.clone(),
                        state: DbSessionState::Published,
                        outcome: Some(ReviewOutcome::Comment.into()),
                        body: None,
                        published_at: None,
                    })
                    .await?;
                session_id_by_forge_id.insert(remote_comment.session_forge_id.clone(), fallback.id);
                fallback.id
            };
            let reply_to_id = remote_comment
                .reply_to_forge_id
                .as_ref()
                .and_then(|fid| comment_id_by_forge_id.get(fid).copied());

            let saved_comment = tx
                .comments
                .upsert_forge_comment(UpsertForgeComment {
                    review_id: db_review.id,
                    session_id,
                    author_id: author_row.id,
                    reply_to_id,
                    forge_id: remote_comment.forge_id.clone(),
                    file_path: remote_comment.file_path,
                    start_line: remote_comment
                        .start_line
                        .and_then(|v| i32::try_from(v).ok()),
                    line: remote_comment.line.and_then(|v| i32::try_from(v).ok()),
                    commit_sha: remote_comment
                        .commit_sha
                        .or_else(|| Some(pr.head_sha.clone())),
                    original_start_line: remote_comment
                        .original_start_line
                        .and_then(|v| i32::try_from(v).ok()),
                    original_line: remote_comment
                        .original_line
                        .and_then(|v| i32::try_from(v).ok()),
                    original_commit_sha: remote_comment
                        .original_commit_sha
                        .or_else(|| Some(pr.head_sha.clone())),
                    body: remote_comment.body,
                    resolved: remote_comment.resolved,
                    outdated: remote_comment.outdated,
                })
                .await?;
            comment_id_by_forge_id.insert(remote_comment.forge_id, saved_comment.id);
        }

        let draft_session = tx
            .sessions
            .get_or_create_draft_session(db_review.id, actor.id, None)
            .await?;
        tx.commit().await?;

        let mut domain_review = Review::from_db(db_review)?;
        let mut updated_anchors = 0u32;

        if let Some(old_sha) = old_head_sha
            && old_sha != pr.head_sha
        {
            let comments = self
                .load_review_comments(domain_review.id, Some(actor.id))
                .await?;
            if let Ok(shifts) =
                self.snapshots
                    .recompute_anchors(git_dir, &old_sha, &pr.head_sha, &comments)
                && !shifts.is_empty()
            {
                updated_anchors = u32::try_from(shifts.len()).unwrap_or(u32::MAX);
                self.database
                    .comments
                    .update_comment_anchors(&shifts)
                    .await?;
                let comment_by_id: HashMap<Uuid, &Comment> =
                    comments.iter().map(|c| (c.id, c)).collect();
                for shift in &shifts {
                    if let Some(comment) = comment_by_id.get(&shift.comment_id)
                        && let Some(forge_id) = &comment.forge_id
                        && let Some(anchor) = &comment.anchor
                    {
                        let _ = self
                            .forge
                            .update_remote_comment(
                                repository_url,
                                forge_id,
                                &ForgeCommentUpdate {
                                    body: None,
                                    file_path: Some(anchor.file_path.clone()),
                                    start_line: shift
                                        .start_line
                                        .and_then(|v| u32::try_from(v).ok()),
                                    line: shift.line.and_then(|v| u32::try_from(v).ok()),
                                    commit_sha: shift.commit_sha.clone(),
                                    resolved: None,
                                },
                            )
                            .await;
                    }
                }
            }
            if let Ok(pruned) =
                self.prune_modified_reviewed_files(git_dir, &domain_review, &pr.head_sha)
            {
                let updated_row = self
                    .database
                    .reviews
                    .update_reviewed_files(domain_review.id, pruned)
                    .await?;
                domain_review = Review::from_db(updated_row)?;
            }
        }

        let domain_session = Session::from_db(draft_session, actor.clone())?;
        Ok((domain_review, domain_session, updated_anchors))
    }

    fn prune_modified_reviewed_files(
        &self,
        git_dir: &str,
        review: &Review,
        new_head_sha: &str,
    ) -> Result<BTreeMap<String, String>> {
        let mut retained = BTreeMap::new();
        let mut changed_by_sha: HashMap<String, HashSet<String>> = HashMap::new();
        for (file_path, viewed_sha) in &review.reviewed_files {
            if viewed_sha == new_head_sha {
                retained.insert(file_path.clone(), viewed_sha.clone());
                continue;
            }
            let changed_set = if let Some(cached) = changed_by_sha.get(viewed_sha) {
                cached
            } else {
                let changed =
                    self.snapshots
                        .get_changed_files(git_dir, viewed_sha, new_head_sha)?;
                changed_by_sha
                    .entry(viewed_sha.clone())
                    .or_insert_with(|| changed.into_iter().collect())
            };
            if !changed_set.contains(file_path) {
                retained.insert(file_path.clone(), viewed_sha.clone());
            }
        }
        Ok(retained)
    }

    async fn load_review_sessions(&self, review_id: Uuid) -> Result<Vec<Session>> {
        let rows = self
            .database
            .sessions
            .get_sessions_by_review(review_id)
            .await?;
        let author_ids: Vec<Uuid> = rows.iter().map(|r| r.author_id).collect();
        let identities = self.load_identities_map(&author_ids).await?;
        let mut sessions = Vec::with_capacity(rows.len());
        for row in rows {
            let author = identities
                .get(&row.author_id)
                .cloned()
                .ok_or_else(|| Error::NotFound("session author not found".to_owned()))?;
            sessions.push(Session::from_db(row, author)?);
        }
        Ok(sessions)
    }

    async fn load_review_comments(
        &self,
        review_id: Uuid,
        viewer_id: Option<Uuid>,
    ) -> Result<Vec<Comment>> {
        let rows = self
            .database
            .comments
            .get_comments_by_review(review_id, viewer_id)
            .await?;
        let author_ids: Vec<Uuid> = rows.iter().map(|r| r.author_id).collect();
        let identities = self.load_identities_map(&author_ids).await?;
        let number_by_id: HashMap<Uuid, u64> = rows
            .iter()
            .filter_map(|r| u64::try_from(r.number).ok().map(|num| (r.id, num)))
            .collect();

        let mut comments = Vec::with_capacity(rows.len());
        for row in rows {
            let author = identities
                .get(&row.author_id)
                .cloned()
                .ok_or_else(|| Error::NotFound("comment author not found".to_owned()))?;
            let reply_to_number = row
                .reply_to_id
                .and_then(|parent_id| number_by_id.get(&parent_id).copied());
            comments.push(Comment::from_db(row, author, reply_to_number)?);
        }
        Ok(comments)
    }

    async fn load_identities_map(&self, ids: &[Uuid]) -> Result<HashMap<Uuid, Identity>> {
        let rows = self.database.identities.get_identities_by_ids(ids).await?;
        let mut map = HashMap::with_capacity(rows.len());
        for row in rows {
            let domain = Identity::from_db(row)?;
            map.insert(domain.id, domain);
        }
        Ok(map)
    }

    async fn load_identity(&self, id: Uuid) -> Result<Identity> {
        let row = self
            .database
            .identities
            .get_identity(id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("identity {id} not found")))?;
        Identity::from_db(row)
    }

    async fn resolve_reply_to_number(&self, reply_to_id: Option<Uuid>) -> Result<Option<u64>> {
        let Some(parent_id) = reply_to_id else {
            return Ok(None);
        };
        let parent = self
            .database
            .comments
            .get_comment(parent_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("parent comment {parent_id} not found")))?;
        Ok(u64::try_from(parent.number).ok())
    }
}

fn order_comments_for_display(mut comments: Vec<Comment>, group_by: ReviewGroupBy) -> Vec<Comment> {
    match group_by {
        ReviewGroupBy::Session => {
            comments.sort_by(|a, b| {
                a.session_id
                    .cmp(&b.session_id)
                    .then_with(|| a.number.cmp(&b.number))
            });
            comments
        }
        ReviewGroupBy::File => {
            let (mut roots, replies): (Vec<Comment>, Vec<Comment>) = comments
                .into_iter()
                .partition(|c| c.reply_to_number.is_none());
            roots.sort_by(|a, b| {
                let a_path = a
                    .anchor
                    .as_ref()
                    .map(|anc| anc.file_path.as_str())
                    .unwrap_or("");
                let b_path = b
                    .anchor
                    .as_ref()
                    .map(|anc| anc.file_path.as_str())
                    .unwrap_or("");
                let a_line = a.anchor.as_ref().map(|anc| anc.line).unwrap_or(0);
                let b_line = b.anchor.as_ref().map(|anc| anc.line).unwrap_or(0);
                a_path
                    .cmp(b_path)
                    .then_with(|| a_line.cmp(&b_line))
                    .then_with(|| a.number.cmp(&b.number))
            });

            let mut replies_by_root: HashMap<u64, Vec<Comment>> = HashMap::new();
            for reply in replies {
                if let Some(root_num) = reply.reply_to_number {
                    replies_by_root.entry(root_num).or_default().push(reply);
                }
            }
            for thread_replies in replies_by_root.values_mut() {
                thread_replies.sort_by_key(|r| r.number);
            }

            let mut ordered = Vec::new();
            for root in roots {
                let root_num = root.number;
                ordered.push(root);
                if let Some(thread_replies) = replies_by_root.remove(&root_num) {
                    ordered.extend(thread_replies);
                }
            }
            ordered
        }
    }
}
