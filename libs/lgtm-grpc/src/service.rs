//! Business logic for every Connect-JSON endpoint.
//!
//! Handlers in [`crate::server`] delegate to [`LgtmService`]; the GPUI app
//! can call the same trait in-process once it swaps its stub.
//!
//! The default implementation owns an `Arc<Db>` so request handlers borrow it
//! without taking the connection out of its Mutex.

use std::path::PathBuf;
use std::sync::Arc;

use lgtm_db_client::{CommentRow, Db, ThreadRow, UserRow};

use crate::error::ServiceError;
use crate::git;
use crate::proto::*;

pub trait LgtmService: Send + Sync + 'static {
    // Diff
    fn get_changed_files(
        &self,
        req: GetChangedFilesRequest,
    ) -> Result<GetChangedFilesResponse, ServiceError>;
    fn get_file_diff(
        &self,
        req: GetFileDiffRequest,
    ) -> Result<GetFileDiffResponse, ServiceError>;

    // Identity
    fn upsert_user(&self, req: UpsertUserRequest) -> Result<UpsertUserResponse, ServiceError>;

    // Comments
    fn list_threads(&self, req: ListThreadsRequest) -> Result<ListThreadsResponse, ServiceError>;
    fn create_thread(&self, req: CreateThreadRequest) -> Result<CreateThreadResponse, ServiceError>;
    fn append_comment(
        &self,
        req: AppendCommentRequest,
    ) -> Result<AppendCommentResponse, ServiceError>;
    fn resolve_thread(
        &self,
        req: ResolveThreadRequest,
    ) -> Result<ResolveThreadResponse, ServiceError>;
}

/// Default implementation. Holds an `Arc<Db>` so the server can serve many
/// concurrent requests against one connection (sqlite behind a Mutex, which
/// is fine at PoC load).
pub struct DefaultService {
    db: Arc<Db>,
}

impl DefaultService {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }
}

impl LgtmService for DefaultService {
    // ----- Diff -----

    fn get_changed_files(
        &self,
        req: GetChangedFilesRequest,
    ) -> Result<GetChangedFilesResponse, ServiceError> {
        validate_repo_path(&req.repo_path)?;
        let repo = git::open(&PathBuf::from(&req.repo_path))?;
        let files = git::changed_files(&repo, &req.base_ref)?;
        Ok(GetChangedFilesResponse { files })
    }

    fn get_file_diff(
        &self,
        req: GetFileDiffRequest,
    ) -> Result<GetFileDiffResponse, ServiceError> {
        validate_repo_path(&req.repo_path)?;
        if req.path.is_empty() {
            return Err(ServiceError::InvalidArgument("path must not be empty".into()));
        }
        let repo = git::open(&PathBuf::from(&req.repo_path))?;
        let diff_text = git::file_diff(&repo, &req.base_ref, &req.path)?;
        Ok(GetFileDiffResponse { diff_text })
    }

    // ----- Identity -----

    fn upsert_user(&self, req: UpsertUserRequest) -> Result<UpsertUserResponse, ServiceError> {
        if req.name.trim().is_empty() {
            return Err(ServiceError::InvalidArgument("name must not be empty".into()));
        }
        let row = self
            .db
            .upsert_user(req.name.trim(), req.kind.as_db_str())
            .map_err(ServiceError::Other)?;
        Ok(UpsertUserResponse { user: user_to_proto(row) })
    }

    // ----- Comments -----

    fn list_threads(
        &self,
        req: ListThreadsRequest,
    ) -> Result<ListThreadsResponse, ServiceError> {
        validate_repo_path(&req.repo_path)?;
        let file_filter = (!req.file_path.is_empty()).then_some(req.file_path.as_str());
        let rows = self
            .db
            .list_threads(&req.repo_path, &req.base_ref, file_filter, req.only_unresolved)
            .map_err(ServiceError::Other)?;
        let threads = rows
            .into_iter()
            .map(|(t, comments)| thread_to_proto(t, comments))
            .collect();
        Ok(ListThreadsResponse { threads })
    }

    fn create_thread(
        &self,
        req: CreateThreadRequest,
    ) -> Result<CreateThreadResponse, ServiceError> {
        validate_repo_path(&req.repo_path)?;
        if req.file_path.trim().is_empty() {
            return Err(ServiceError::InvalidArgument("file_path must not be empty".into()));
        }
        if req.body.trim().is_empty() {
            return Err(ServiceError::InvalidArgument("body must not be empty".into()));
        }
        if req.line < 0 {
            return Err(ServiceError::InvalidArgument("line must be >= 0".into()));
        }
        let (thread, comment) = self
            .db
            .create_thread(
                &req.repo_path,
                &req.base_ref,
                req.file_path.trim(),
                req.line,
                req.author_id,
                req.body.trim(),
            )
            .map_err(ServiceError::Other)?;
        Ok(CreateThreadResponse {
            thread: thread_to_proto(thread, vec![comment]),
        })
    }

    fn append_comment(
        &self,
        req: AppendCommentRequest,
    ) -> Result<AppendCommentResponse, ServiceError> {
        if req.body.trim().is_empty() {
            return Err(ServiceError::InvalidArgument("body must not be empty".into()));
        }
        let row = self
            .db
            .append_comment(req.thread_id, req.author_id, req.body.trim())
            .map_err(ServiceError::Other)?;
        Ok(AppendCommentResponse { comment: comment_to_proto(row) })
    }

    fn resolve_thread(
        &self,
        req: ResolveThreadRequest,
    ) -> Result<ResolveThreadResponse, ServiceError> {
        let thread_row = self
            .db
            .resolve_thread(req.thread_id)
            .map_err(ServiceError::Other)?;
        // Fetch comments so UIs can refresh without a round-trip.
        let threads = self
            .db
            .list_threads(
                &thread_row.repo_path,
                &thread_row.base_ref,
                Some(&thread_row.file_path),
                false,
            )
            .map_err(ServiceError::Other)?;
        let comments = threads
            .into_iter()
            .find(|(t, _)| t.id == thread_row.id)
            .map(|(_, c)| c)
            .unwrap_or_default();
        Ok(ResolveThreadResponse {
            thread: thread_to_proto(thread_row, comments),
        })
    }
}

// ---- Row -> proto converters ----

fn user_to_proto(r: UserRow) -> User {
    User {
        id: r.id,
        name: r.name,
        kind: AuthorKind::from_db_str(&r.kind),
        created_at: r.created_at,
    }
}

fn comment_to_proto(r: CommentRow) -> Comment {
    Comment {
        id: r.id,
        thread_id: r.thread_id,
        body: r.body,
        created_at: r.created_at,
        author: user_to_proto(r.author),
    }
}

fn thread_to_proto(t: ThreadRow, comments: Vec<CommentRow>) -> Thread {
    Thread {
        id: t.id,
        repo_path: t.repo_path,
        base_ref: t.base_ref,
        file_path: t.file_path,
        line: t.line,
        resolved: t.resolved,
        created_at: t.created_at,
        comments: comments.into_iter().map(comment_to_proto).collect(),
    }
}

fn validate_repo_path(p: &str) -> Result<(), ServiceError> {
    if p.is_empty() {
        return Err(ServiceError::InvalidArgument("repo_path must not be empty".into()));
    }
    Ok(())
}
