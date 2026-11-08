//! Domain models for workspace diff inspection, commit range resolution, and review mode selection.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use specta::Type;

/// Active review mode controlling whether the desktop diff viewer inspects local LGTM snapshots and the working tree or standard Git branch commits.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ReviewMode {
    /// Local review mode including uncommitted working-tree changes and `~/.lgtm/snapshots/$repoSlug` shadow commits.
    #[default]
    Local,
    /// Forge-backed review mode listing standard Git commits on the active branch in `<workspace>/.git`.
    ForgeBacked,
}

/// Source repository or checkout layer from which a commit checklist item originates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DiffCommitSource {
    /// Live working-tree files on disk compared against the latest snapshot or branch `HEAD`.
    WorkingTree,
    /// Shadow commit recorded on `refs/lgtm/snapshots/<branch>` inside `~/.lgtm/snapshots/$repoSlug`.
    LocalSnapshot,
    /// Standard Git commit recorded in `<workspace>/.git`.
    GitCommit,
}

/// Selectable commit or working-tree entry displayed in the bottom diff sidebar checklist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DiffCommitItem {
    /// Commit identifier (`"working_tree"` or a 40-character hex commit SHA).
    pub id: String,
    /// Abbreviated commit label (`"WORKTREE"` or a 7-character short SHA).
    pub short_sha: String,
    /// Single-line commit subject or `"Working Tree"`.
    pub title: String,
    /// Commit author name.
    pub author: String,
    /// Commit author Unix timestamp in seconds.
    pub timestamp: i64,
    /// First-parent commit SHA (`None` for an initial root commit).
    pub parent_sha: Option<String>,
    /// Origin of this commit entry.
    pub source: DiffCommitSource,
}

impl DiffCommitItem {
    // Public ------------------------------------------------------------------

    /// Orders `selected_ids` according to the newest-first `commits` list and returns `(oldest.parent_sha, newest.id)`.
    pub fn resolve_commit_range(
        commits: &[Self],
        selected_ids: &[String],
    ) -> Option<(String, String)> {
        let selected_set: BTreeSet<&str> = selected_ids
            .iter()
            .map(String::as_str)
            .filter(|id| !id.trim().is_empty())
            .collect();
        if selected_set.is_empty() {
            return None;
        }
        let matching: Vec<&Self> = commits
            .iter()
            .filter(|item| selected_set.contains(item.id.as_str()))
            .collect();
        let newest = matching.first()?;
        let oldest = matching.last()?;
        let base_sha = oldest.parent_sha.clone().unwrap_or_default();
        Some((base_sha, newest.id.clone()))
    }
}

/// Change classification for a file in a diff range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DiffFileStatus {
    /// File was created in the head tree.
    Added,
    /// File exists in both trees with modified content.
    Modified,
    /// File existed in the base tree and was removed in the head tree.
    Deleted,
    /// File was moved or renamed from `old_path` to `path`.
    Renamed,
}

/// Summary of a changed file and its line delta counts for the diff explorer tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DiffFileEntry {
    /// Workspace-relative file path in the head tree (or base tree when deleted).
    pub path: String,
    /// Previous workspace-relative path when `status` is [`DiffFileStatus::Renamed`].
    pub old_path: Option<String>,
    /// File change classification (`added`, `modified`, `deleted`, or `renamed`).
    pub status: DiffFileStatus,
    /// Number of added lines in the file diff.
    pub additions: u32,
    /// Number of deleted lines in the file diff.
    pub deletions: u32,
}

/// Complete base and head UTF-8 text payload for a single file across a selected commit range.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FileDiffContent {
    /// Workspace-relative file path.
    pub path: String,
    /// Previous workspace-relative file path when renamed.
    pub old_path: Option<String>,
    /// File change classification.
    pub status: DiffFileStatus,
    /// Resolved base commit SHA (or `""` when comparing against an empty root tree).
    pub base_sha: String,
    /// Resolved head commit ID (`"working_tree"` or a commit SHA).
    pub head_sha: String,
    /// UTF-8 content of the file at `base_sha` (empty when added or binary).
    pub base_content: String,
    /// UTF-8 content of the file at `head_sha` (empty when deleted or binary).
    pub head_content: String,
    /// Whether either the base or head blob contains non-UTF-8 binary bytes.
    pub is_binary: bool,
    /// Whether the head buffer is read-only (`false` only when `head_sha == "working_tree"`).
    pub readonly: bool,
}

/// Snapshot of the active workspace diff state returned on initial load, mode toggle, or workspace refresh.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DiffWorkspaceSnapshot {
    /// Active review mode (`local` or `forge_backed`).
    pub review_mode: ReviewMode,
    /// Short name of the checked-out Git branch (or `"HEAD"` when detached).
    pub branch: String,
    /// Ordered newest-first list of commits available in the bottom checklist.
    pub commits: Vec<DiffCommitItem>,
    /// Currently checked commit IDs in the checklist.
    pub selected_commit_ids: Vec<String>,
    /// Changed files across `selected_commit_ids`.
    pub files: Vec<DiffFileEntry>,
}

// tests -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // fixtures ----------------------------------------------------------------

    fn sample_commits() -> Vec<DiffCommitItem> {
        vec![
            DiffCommitItem {
                id: "working_tree".to_string(),
                short_sha: "WORKTREE".to_string(),
                title: "Working Tree".to_string(),
                author: "LGTM Reviewer".to_string(),
                timestamp: 1_700_000_040,
                parent_sha: Some("2222222222222222222222222222222222222222".to_string()),
                source: DiffCommitSource::WorkingTree,
            },
            DiffCommitItem {
                id: "2222222222222222222222222222222222222222".to_string(),
                short_sha: "2222222".to_string(),
                title: "lgtm: refine beta calculation".to_string(),
                author: "LGTM Reviewer".to_string(),
                timestamp: 1_700_000_030,
                parent_sha: Some("1111111111111111111111111111111111111111".to_string()),
                source: DiffCommitSource::LocalSnapshot,
            },
            DiffCommitItem {
                id: "1111111111111111111111111111111111111111".to_string(),
                short_sha: "1111111".to_string(),
                title: "feat: initial workspace".to_string(),
                author: "LGTM Reviewer".to_string(),
                timestamp: 1_700_000_010,
                parent_sha: None,
                source: DiffCommitSource::GitCommit,
            },
        ]
    }

    // tests -------------------------------------------------------------------

    mod given_a_commit_checklist {
        use super::*;

        mod when_resolving_commit_range {
            use super::*;

            #[test]
            fn should_resolve_oldest_parent_and_newest_head_regardless_of_selection_order() {
                // Arrange
                let commits = sample_commits();
                let selected = vec![
                    "1111111111111111111111111111111111111111".to_string(),
                    "2222222222222222222222222222222222222222".to_string(),
                ];

                // Act
                let single =
                    DiffCommitItem::resolve_commit_range(&commits, &["working_tree".to_string()]);
                let multi = DiffCommitItem::resolve_commit_range(&commits, &selected);

                // Assert
                assert_eq!(
                    single,
                    Some((
                        "2222222222222222222222222222222222222222".to_string(),
                        "working_tree".to_string(),
                    ))
                );
                assert_eq!(
                    multi,
                    Some((
                        String::new(),
                        "2222222222222222222222222222222222222222".to_string(),
                    ))
                );
            }
        }
    }
}
