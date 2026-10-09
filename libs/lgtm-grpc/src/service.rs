//! Business logic for the two Connect-JSON endpoints. The handlers in
//! [`crate::server`] delegate here so the surface area tested by HTTP tests
//! and in-process callers (e.g. the GPUI app) is identical.

use std::path::PathBuf;

use crate::error::ServiceError;
use crate::git;
use crate::proto::*;

pub trait LgtmService: Send + Sync + 'static {
    fn get_changed_files(
        &self,
        req: GetChangedFilesRequest,
    ) -> Result<GetChangedFilesResponse, ServiceError>;

    fn get_file_diff(
        &self,
        req: GetFileDiffRequest,
    ) -> Result<GetFileDiffResponse, ServiceError>;
}

/// Default implementation backed by `git2`. Keep this struct empty of state so
/// handler-side cloning via `Arc<DefaultService>` is cheap.
#[derive(Default, Clone)]
pub struct DefaultService;

impl LgtmService for DefaultService {
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
}

fn validate_repo_path(p: &str) -> Result<(), ServiceError> {
    if p.is_empty() {
        return Err(ServiceError::InvalidArgument("repo_path must not be empty".into()));
    }
    Ok(())
}
