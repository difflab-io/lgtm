//! Shared integration test fixtures (`TestContext`, temporary Git checkout, `SnapshotStore`, and `DummyForge`) for `lgtm-forge`.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
};

use chrono::Utc;
use lgtm_forge::{
    database::Database,
    error::{Error, Result},
    forge::{
        ForgeCommentUpdate, ForgeDraftCommentInput, ForgeDraftCommentReceipt, ForgePullRequest,
        ForgePullRequestBundle, ForgeReviewComment, ForgeReviewReceipt, ForgeReviewSession,
        RemoteForge,
    },
    models::{ReviewOutcome, ReviewStatus},
    services::ReviewService,
    snapshots::SnapshotStore,
};
use tempfile::TempDir;

// fixtures --------------------------------------------------------------------

/// Recorded call log and programmable state for `DummyForge`.
#[derive(Clone, Debug, Default)]
pub struct DummyForgeState {
    pub bundle: Option<ForgePullRequestBundle>,
    pub fail_draft_comment_sync: bool,
    pub created_draft_comments: Vec<(u64, Option<String>, ForgeDraftCommentInput)>,
    pub updated_comments: Vec<(String, ForgeCommentUpdate)>,
    pub deleted_comments: Vec<String>,
    pub viewed_files: Vec<(Option<String>, u64, String, bool)>,
    pub submitted_reviews: Vec<(u64, Option<String>, ReviewOutcome, Option<String>)>,
    pub next_id: u64,
}

/// Deterministic in-memory `RemoteForge` test double.
#[derive(Clone, Debug, Default)]
pub struct DummyForge {
    state: Arc<Mutex<DummyForgeState>>,
}

impl DummyForge {
    /// Construct a new `DummyForge`.
    pub fn new() -> Self {
        Self::default()
    }

    // Public ------------------------------------------------------------------

    /// Configure the `ForgePullRequestBundle` returned by `get_pull_request_bundle`.
    pub fn set_bundle(&self, bundle: ForgePullRequestBundle) {
        self.state.lock().expect("lock dummy forge").bundle = Some(bundle);
    }

    /// Toggle simulated network failure on `create_remote_draft_comment` so unsynced comment flushing can be tested.
    pub fn set_fail_draft_comment_sync(&self, fail: bool) {
        self.state
            .lock()
            .expect("lock dummy forge")
            .fail_draft_comment_sync = fail;
    }

    /// Return a snapshot of all calls recorded by `DummyForge`.
    pub fn state(&self) -> DummyForgeState {
        self.state.lock().expect("lock dummy forge").clone()
    }
}

#[async_trait::async_trait]
impl RemoteForge for DummyForge {
    async fn get_pull_request_bundle(
        &self,
        _repository_url: &str,
        _branch_or_pr: &str,
    ) -> Result<ForgePullRequestBundle> {
        let state = self.state.lock().expect("lock dummy forge");
        state
            .bundle
            .clone()
            .ok_or_else(|| Error::NotFound("no dummy PR bundle configured".to_owned()))
    }

    async fn create_pending_review(
        &self,
        _repository_url: &str,
        pr_number: u64,
        _head_sha: &str,
    ) -> Result<String> {
        let mut state = self.state.lock().expect("lock dummy forge");
        state.next_id += 1;
        Ok(format!("PRR_dummy_{pr_number}_{}", state.next_id))
    }

    async fn create_remote_draft_comment(
        &self,
        _repository_url: &str,
        pr_number: u64,
        pending_review_id: Option<&str>,
        comment: &ForgeDraftCommentInput,
    ) -> Result<ForgeDraftCommentReceipt> {
        let mut state = self.state.lock().expect("lock dummy forge");
        if state.fail_draft_comment_sync {
            return Err(Error::Forge("simulated transient network error".to_owned()));
        }
        state.next_id += 1;
        let review_id = pending_review_id
            .map(str::to_owned)
            .unwrap_or_else(|| format!("PRR_dummy_{pr_number}_{}", state.next_id));
        let comment_id = format!("PRRC_dummy_{}", state.next_id);
        state.created_draft_comments.push((
            pr_number,
            pending_review_id.map(str::to_owned),
            comment.clone(),
        ));
        Ok(ForgeDraftCommentReceipt {
            pending_review_forge_id: review_id,
            comment_forge_id: comment_id,
        })
    }

    async fn update_remote_comment(
        &self,
        _repository_url: &str,
        forge_id: &str,
        update: &ForgeCommentUpdate,
    ) -> Result<()> {
        let mut state = self.state.lock().expect("lock dummy forge");
        state
            .updated_comments
            .push((forge_id.to_owned(), update.clone()));
        Ok(())
    }

    async fn delete_remote_comment(&self, _repository_url: &str, forge_id: &str) -> Result<()> {
        let mut state = self.state.lock().expect("lock dummy forge");
        state.deleted_comments.push(forge_id.to_owned());
        Ok(())
    }

    async fn set_remote_file_viewed(
        &self,
        _repository_url: &str,
        pr_forge_id: Option<&str>,
        pr_number: u64,
        file_path: &str,
        viewed: bool,
    ) -> Result<()> {
        let mut state = self.state.lock().expect("lock dummy forge");
        state.viewed_files.push((
            pr_forge_id.map(str::to_owned),
            pr_number,
            file_path.to_owned(),
            viewed,
        ));
        Ok(())
    }

    async fn submit_pending_review(
        &self,
        _repository_url: &str,
        pr_number: u64,
        pending_review_id: Option<&str>,
        outcome: ReviewOutcome,
        body: Option<&str>,
    ) -> Result<ForgeReviewReceipt> {
        let mut state = self.state.lock().expect("lock dummy forge");
        state.next_id += 1;
        let review_id = pending_review_id
            .map(str::to_owned)
            .unwrap_or_else(|| format!("PRR_submitted_{pr_number}_{}", state.next_id));
        state.submitted_reviews.push((
            pr_number,
            pending_review_id.map(str::to_owned),
            outcome,
            body.map(str::to_owned),
        ));
        Ok(ForgeReviewReceipt {
            review_forge_id: review_id,
            published_at: Utc::now(),
        })
    }
}

/// Self-contained test harness holding a temporary Git checkout, `SnapshotStore`, migrated `Database`, and `DummyForge`.
pub struct TestContext {
    _temp_dir: TempDir,
    pub worktree_dir: PathBuf,
    pub git_dir: String,
    pub database: Database,
    pub snapshots: SnapshotStore,
    pub forge: Arc<DummyForge>,
    pub service: ReviewService,
}

impl TestContext {
    /// Provision an isolated Git working checkout, `SnapshotStore`, migrated SQLite/PostgreSQL `Database`, and `ReviewService`.
    pub async fn setup() -> Self {
        let temp_dir = TempDir::new().expect("create temp dir");
        let worktree_dir = temp_dir.path().join("repo");
        let snapshots_root = temp_dir.path().join("snapshots");
        std::fs::create_dir_all(&worktree_dir).expect("create repo dir");

        run_git(&worktree_dir, &["init", "-b", "feature/diff-241"]);
        run_git(&worktree_dir, &["config", "user.name", "Ada Lovelace"]);
        run_git(&worktree_dir, &["config", "user.email", "ada@example.com"]);
        run_git(
            &worktree_dir,
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/difflab-io/lgtm.git",
            ],
        );

        write_repo_file(
            &worktree_dir,
            "src/lib.rs",
            "pub fn alpha() -> i32 {\n    1\n}\n\npub fn beta() -> i32 {\n    2\n}\n\npub fn gamma() -> i32 {\n    3\n}\n",
        );
        write_repo_file(&worktree_dir, "README.md", "# LGTM\nInitial readme.\n");
        run_git(&worktree_dir, &["add", "."]);
        run_git(&worktree_dir, &["commit", "-m", "Initial commit"]);

        let git_dir_path =
            std::fs::canonicalize(worktree_dir.join(".git")).expect("canonical .git");
        let git_dir = git_dir_path.to_string_lossy().into_owned();

        let database = setup_database(&temp_dir).await;
        let snapshots = SnapshotStore::new(&snapshots_root);
        let forge = Arc::new(DummyForge::new());
        let service =
            ReviewService::with_dependencies(database.clone(), snapshots.clone(), forge.clone());

        Self {
            _temp_dir: temp_dir,
            worktree_dir,
            git_dir,
            database,
            snapshots,
            forge,
            service,
        }
    }

    // Public ------------------------------------------------------------------

    /// Write `contents` to `rel_path` inside the test working tree.
    pub fn write_file(&self, rel_path: &str, contents: &str) {
        write_repo_file(&self.worktree_dir, rel_path, contents);
    }

    /// Return the current `HEAD` commit SHA of the working checkout.
    pub fn checkout_head_sha(&self) -> String {
        git_stdout(&self.worktree_dir, &["rev-parse", "HEAD"])
    }

    /// Return the raw output of `git status --porcelain` in the working checkout.
    pub fn checkout_status_porcelain(&self) -> String {
        git_stdout(&self.worktree_dir, &["status", "--porcelain"])
    }

    /// Build a sample `ForgePullRequestBundle` for `pr_number` at `head_sha`.
    pub fn sample_pr_bundle(
        &self,
        pr_number: u64,
        base_sha: &str,
        head_sha: &str,
        sessions: Vec<ForgeReviewSession>,
        comments: Vec<ForgeReviewComment>,
    ) -> ForgePullRequestBundle {
        ForgePullRequestBundle {
            pull_request: ForgePullRequest {
                pr_number,
                forge_id: format!("PR_node_{pr_number}"),
                branch: "feature/diff-241".to_owned(),
                base_sha: base_sha.to_owned(),
                head_sha: head_sha.to_owned(),
                status: ReviewStatus::Open,
                reviewed_files: BTreeMap::new(),
            },
            sessions,
            comments,
        }
    }
}

async fn setup_database(temp_dir: &TempDir) -> Database {
    if let Ok(pg_url) = std::env::var("LGTM_TEST_POSTGRES_URL")
        && !pg_url.trim().is_empty()
    {
        return Database::connect(&pg_url)
            .await
            .expect("connect test postgres");
    }
    let db_path = temp_dir.path().join("test.db");
    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    Database::connect(&url).await.expect("connect test sqlite")
}

fn write_repo_file(worktree_dir: &Path, rel_path: &str, contents: &str) {
    let target = worktree_dir.join(rel_path);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).expect("create parent dirs");
    }
    std::fs::write(target, contents).expect("write repo file");
}

fn run_git(worktree_dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(worktree_dir)
        .args(args)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_stdout(worktree_dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(worktree_dir)
        .args(args)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .trim_end_matches(&['\r', '\n'][..])
        .to_owned()
}
