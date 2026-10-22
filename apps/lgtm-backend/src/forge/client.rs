//! Typed provider contract for PR discovery and complete discussion snapshots.
use super::models::{ForgeSnapshot, PullRequest, Repository};
use crate::error::Result;

/// Provider reads used by review orchestration. Implementations normalize forge data and return complete snapshots only after pagination finishes.
/// Provider-specific REST/GraphQL writes remain on the concrete client.
pub trait ForgeClient: Clone + Send + Sync {
    fn get_pull_requests(
        &self,
        repo: &Repository,
        query: &str,
        page: u32,
    ) -> impl std::future::Future<Output = Result<(Vec<PullRequest>, u32)>> + Send;

    fn get_pull_request(
        &self,
        repo: &Repository,
        pr: i64,
    ) -> impl std::future::Future<Output = Result<PullRequest>> + Send;

    fn get_account_id(&self) -> impl std::future::Future<Output = Result<String>> + Send;

    fn get_snapshot(
        &self,
        repo: &Repository,
        pr: i64,
    ) -> impl std::future::Future<Output = Result<ForgeSnapshot>> + Send;
}
