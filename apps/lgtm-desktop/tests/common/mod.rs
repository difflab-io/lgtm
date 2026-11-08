//! Shared Git and LGTM shadow-snapshot fixture builders with a declarative 5-stage visual storyboard (`C1` -> `C2` -> `S1` -> `S2` -> `WT`).
//!
//! ```text
//! Stage-by-Stage Evolution of `src/lib.rs`:
//! ┌───────────────────────────┬───────────────────────────┬───────────────────────────┬───────────────────────────┐
//! │ Stage 1 (C1: Base)        │ Stage 2 (C2: Git HEAD)    │ Stage 4 (S2: Local Snap)  │ Stage 5 (WT: WorkingTree) │
//! ├───────────────────────────┼───────────────────────────┼───────────────────────────┼───────────────────────────┤
//! │ pub fn alpha() -> i32 {   │ pub fn alpha() -> i32 {   │ pub fn alpha() -> i32 {   │ pub fn alpha() -> i32 {   │
//! │     1                     │     10 // modified in C2  │     10 // modified in C2  │     10 // modified in C2  │
//! │ }                         │ }                         │ }                         │ }                         │
//! │                           │                           │                           │                           │
//! │ pub fn beta() -> i32 {    │ pub fn beta() -> i32 {    │ pub fn beta() -> i32 {    │ pub fn beta() -> i32 {    │
//! │     2                     │     2                     │     20 // modified in S2  │     20 // modified in S2  │
//! │ }                         │ }                         │ }                         │ }                         │
//! │                           │                           │                           │                           │
//! │ pub fn gamma() -> i32 {   │ pub fn gamma() -> i32 {   │ pub fn gamma() -> i32 {   │ pub fn gamma() -> i32 {   │
//! │     3                     │     3                     │     3                     │     30 // modified in WT  │
//! │ }                         │ }                         │ }                         │ }                         │
//! └───────────────────────────┴───────────────────────────┴───────────────────────────┴───────────────────────────┘
//!
//! Expected `DiffExplorer` Hierarchical Tree Per Single Stage:
//! ┌──────────────────────┬───────────────────────────────────────────────────────┐
//! │ Stage                │ Expected Collapsible Folder & File Badges             │
//! ├──────────────────────┼───────────────────────────────────────────────────────┤
//! │ Stage 1 (C1)         │ v docs/architecture                                   │
//! │ feat: initial        │   A index.md                                 +2  -0   │
//! │ workspace            │ v src                                                 │
//! │                      │   A lib.rs                                  +11  -0   │
//! │                      │   A old.rs                                   +2  -0   │
//! ├──────────────────────┼───────────────────────────────────────────────────────┤
//! │ Stage 2 (C2)         │ v docs/architecture                                   │
//! │ feat: update alpha   │   M index.md                                 +1  -0   │
//! │ and docs             │ v src                                                 │
//! │                      │   M lib.rs                                   +1  -1   │
//! ├──────────────────────┼───────────────────────────────────────────────────────┤
//! │ Stage 3 (S1)         │ v src                                                 │
//! │ lgtm: add diff       │   v services                                          │
//! │ service              │     A diff.rs                                +4  -0   │
//! │                      │   D old.rs                                   +0  -2   │
//! ├──────────────────────┼───────────────────────────────────────────────────────┤
//! │ Stage 4 (S2)         │ v src                                                 │
//! │ lgtm: refine beta    │   v services                                          │
//! │ calculation          │     M diff.rs                                +2  -1   │
//! │                      │   M lib.rs                                   +1  -1   │
//! ├──────────────────────┼───────────────────────────────────────────────────────┤
//! │ Stage 5 (WT)         │ v docs/architecture                                   │
//! │ Working Tree         │   A diffs.md                                 +3  -0   │
//! │                      │ v src                                                 │
//! │                      │   M lib.rs                                   +1  -1   │
//! └──────────────────────┴───────────────────────────────────────────────────────┘
//! ```

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use gix::{
    objs::{
        Commit, Tree,
        tree::{Entry, EntryKind},
    },
    refs::transaction::{Change, LogChange, PreviousValue, RefEdit, RefLog},
};
use lgtm_desktop::DiffService;
use tempfile::TempDir;

/// Declarative file-content snapshot for a single stage in the 5-stage visual diff storyboard.
#[derive(Debug, Clone, Copy)]
pub struct DiffStageSpec {
    /// Short stage identifier (`"C1"`, `"C2"`, `"S1"`, `"S2"`, or `"WT"`).
    pub code: &'static str,
    /// Commit message or `"Working Tree"`.
    pub title: &'static str,
    /// Complete map of `(relative_path, utf8_content)` present at this stage.
    pub files: &'static [(&'static str, &'static str)],
}

/// Stage 1 (`C1`): `feat: initial workspace` on `refs/heads/feature/diffs` in `<workspace>/.git`.
pub const STAGE_C1_FILES: &[(&str, &str)] = &[
    (
        "docs/architecture/index.md",
        "# Architecture\nOverview of LGTM.\n",
    ),
    (
        "src/lib.rs",
        "pub fn alpha() -> i32 {\n    1\n}\n\npub fn beta() -> i32 {\n    2\n}\n\npub fn gamma() -> i32 {\n    3\n}\n",
    ),
    (
        "src/old.rs",
        "pub const LEGACY_FLAG: bool = true;\npub fn old_helper() {}\n",
    ),
];

/// Stage 2 (`C2`): `feat: update alpha and docs` (`HEAD` in `.git` and `refs/lgtm/heads/feature/diffs` anchor).
pub const STAGE_C2_FILES: &[(&str, &str)] = &[
    (
        "docs/architecture/index.md",
        "# Architecture\nOverview of LGTM.\nUpdated in C2.\n",
    ),
    (
        "src/lib.rs",
        "pub fn alpha() -> i32 {\n    10 // modified in C2\n}\n\npub fn beta() -> i32 {\n    2\n}\n\npub fn gamma() -> i32 {\n    3\n}\n",
    ),
    (
        "src/old.rs",
        "pub const LEGACY_FLAG: bool = true;\npub fn old_helper() {}\n",
    ),
];

/// Stage 3 (`S1`): `lgtm: add diff service` in `~/.lgtm/snapshots/$repoSlug` (`refs/lgtm/snapshots/feature/diffs~1`).
pub const STAGE_S1_FILES: &[(&str, &str)] = &[
    (
        "docs/architecture/index.md",
        "# Architecture\nOverview of LGTM.\nUpdated in C2.\n",
    ),
    (
        "src/lib.rs",
        "pub fn alpha() -> i32 {\n    10 // modified in C2\n}\n\npub fn beta() -> i32 {\n    2\n}\n\npub fn gamma() -> i32 {\n    3\n}\n",
    ),
    (
        "src/services/diff.rs",
        "pub struct DiffService;\npub fn compute_diff() -> usize {\n    1\n}\n",
    ),
];

/// Stage 4 (`S2`): `lgtm: refine beta calculation` in `~/.lgtm/snapshots/$repoSlug` (`refs/lgtm/snapshots/feature/diffs`).
pub const STAGE_S2_FILES: &[(&str, &str)] = &[
    (
        "docs/architecture/index.md",
        "# Architecture\nOverview of LGTM.\nUpdated in C2.\n",
    ),
    (
        "src/lib.rs",
        "pub fn alpha() -> i32 {\n    10 // modified in C2\n}\n\npub fn beta() -> i32 {\n    20 // modified in S2\n}\n\npub fn gamma() -> i32 {\n    3\n}\n",
    ),
    (
        "src/services/diff.rs",
        "pub struct DiffService;\npub fn compute_diff() -> usize {\n    2 // refined in S2\n}\npub fn is_ready() -> bool { true }\n",
    ),
];

/// Stage 5 (`WT`): Live uncommitted working tree files on disk inside `<workspace>`.
pub const STAGE_WT_FILES: &[(&str, &str)] = &[
    (
        "docs/architecture/diffs.md",
        "# Diffs\nVisual diff architecture.\nStage 5 working tree note.\n",
    ),
    (
        "docs/architecture/index.md",
        "# Architecture\nOverview of LGTM.\nUpdated in C2.\n",
    ),
    (
        "src/lib.rs",
        "pub fn alpha() -> i32 {\n    10 // modified in C2\n}\n\npub fn beta() -> i32 {\n    20 // modified in S2\n}\n\npub fn gamma() -> i32 {\n    30 // modified in WT\n}\n",
    ),
    (
        "src/services/diff.rs",
        "pub struct DiffService;\npub fn compute_diff() -> usize {\n    2 // refined in S2\n}\npub fn is_ready() -> bool { true }\n",
    ),
];

/// Ordered 5-stage visual storyboard (`C1`, `C2`, `S1`, `S2`, `WT`).
pub const DIFF_FIXTURE_STAGES: &[DiffStageSpec] = &[
    DiffStageSpec {
        code: "C1",
        title: "feat: initial workspace",
        files: STAGE_C1_FILES,
    },
    DiffStageSpec {
        code: "C2",
        title: "feat: update alpha and docs",
        files: STAGE_C2_FILES,
    },
    DiffStageSpec {
        code: "S1",
        title: "lgtm: add diff service",
        files: STAGE_S1_FILES,
    },
    DiffStageSpec {
        code: "S2",
        title: "lgtm: refine beta calculation",
        files: STAGE_S2_FILES,
    },
    DiffStageSpec {
        code: "WT",
        title: "Working Tree",
        files: STAGE_WT_FILES,
    },
];

/// Seeded workspace and snapshot store handles for integration tests.
pub struct SeededDiffWorkspace {
    /// Temporary directory guard owning both `<home>` and `<workspace>`.
    pub _temp: TempDir,
    /// Canonical `<home>/.lgtm` directory containing `snapshots/$repoSlug`.
    pub lgtm_home: PathBuf,
    /// Canonical `<workspace>` root directory containing `.git` and Stage 5 (`WT`) files.
    pub workspace_root: PathBuf,
    /// Computed `$repoSlug` shared between `lgtm-forge` and `lgtm-desktop`.
    pub repo_slug: String,
    /// Commit SHA of Stage 1 (`C1`) in `<workspace>/.git`.
    pub c1_sha: String,
    /// Commit SHA of Stage 2 (`C2`, `HEAD`) in `<workspace>/.git`.
    pub c2_sha: String,
    /// Commit SHA of Stage 3 (`S1`) in `~/.lgtm/snapshots/$repoSlug`.
    pub s1_sha: String,
    /// Commit SHA of Stage 4 (`S2`) in `~/.lgtm/snapshots/$repoSlug`.
    pub s2_sha: String,
}

/// Seeds the 5-stage visual storyboard (`C1`, `C2` in `.git`; `S1`, `S2` in `~/.lgtm/snapshots/$repoSlug`; `WT` on disk).
pub fn seed_five_stage_workspace() -> SeededDiffWorkspace {
    let temp = tempfile::tempdir().expect("create tempdir");
    let home = temp.path().join("home");
    let lgtm_home = home.join(".lgtm");
    let workspace_raw = temp.path().join("demo-repo");
    std::fs::create_dir_all(&lgtm_home).expect("create .lgtm home");
    std::fs::create_dir_all(&workspace_raw).expect("create workspace");

    gix::init(&workspace_raw).expect("init git repo");
    let workspace_root = workspace_raw.canonicalize().expect("canonical workspace");
    append_git_user_config(&workspace_root.join(".git").join("config"));
    let git_repo = gix::open(&workspace_root).expect("open git repo with user config");

    // Write C1 and C2 into <workspace>/.git on refs/heads/feature/diffs
    let c1_oid = write_stage_commit(
        &git_repo,
        STAGE_C1_FILES,
        vec![],
        DIFF_FIXTURE_STAGES[0].title,
        1_700_000_010,
    );
    let c2_oid = write_stage_commit(
        &git_repo,
        STAGE_C2_FILES,
        vec![c1_oid],
        DIFF_FIXTURE_STAGES[1].title,
        1_700_000_020,
    );
    set_ref(&git_repo, "refs/heads/feature/diffs", c2_oid);
    std::fs::write(
        workspace_root.join(".git").join("HEAD"),
        "ref: refs/heads/feature/diffs\n",
    )
    .expect("point HEAD at feature/diffs");

    // Compute $repoSlug and initialize bare snapshot repo at ~/.lgtm/snapshots/$repoSlug
    let repo_slug = DiffService::compute_repo_slug(&workspace_root).expect("compute repo slug");
    let snapshot_dir = lgtm_home.join("snapshots").join(&repo_slug);
    std::fs::create_dir_all(&snapshot_dir).expect("create snapshot dir");
    gix::init_bare(&snapshot_dir).expect("init bare snapshot repo");
    append_git_user_config(&snapshot_dir.join("config"));
    let snapshot_repo = gix::open(&snapshot_dir).expect("open bare snapshot repo with user config");

    // Set anchor ref `refs/lgtm/heads/feature/diffs` -> C2 and write S1 and S2 on `refs/lgtm/snapshots/feature/diffs`
    set_ref(&snapshot_repo, "refs/lgtm/heads/feature/diffs", c2_oid);
    let s1_oid = write_stage_commit(
        &snapshot_repo,
        STAGE_S1_FILES,
        vec![c2_oid],
        DIFF_FIXTURE_STAGES[2].title,
        1_700_000_030,
    );
    let s2_oid = write_stage_commit(
        &snapshot_repo,
        STAGE_S2_FILES,
        vec![s1_oid],
        DIFF_FIXTURE_STAGES[3].title,
        1_700_000_040,
    );
    set_ref(&snapshot_repo, "refs/lgtm/snapshots/feature/diffs", s2_oid);

    // Materialize Stage 5 (WT) files on disk
    write_worktree_stage(&workspace_root, STAGE_WT_FILES);

    SeededDiffWorkspace {
        _temp: temp,
        lgtm_home,
        workspace_root,
        repo_slug,
        c1_sha: c1_oid.to_hex().to_string(),
        c2_sha: c2_oid.to_hex().to_string(),
        s1_sha: s1_oid.to_hex().to_string(),
        s2_sha: s2_oid.to_hex().to_string(),
    }
}

/// Writes an extra commit onto `repo` with the provided `files`, `parents`, and `title`.
pub fn write_stage_commit(
    repo: &gix::Repository,
    files: &[(&str, &str)],
    parents: Vec<gix::ObjectId>,
    title: &str,
    timestamp: i64,
) -> gix::ObjectId {
    let mut entries = BTreeMap::new();
    for (path, content) in files {
        let blob_id = repo
            .write_blob(content.as_bytes())
            .expect("write blob")
            .detach();
        entries.insert((*path).to_string(), blob_id);
    }
    let tree_oid = write_subtree(repo, &entries, "");
    let sig = gix::actor::Signature {
        name: "LGTM Reviewer".into(),
        email: "reviewer@lgtm.local".into(),
        time: gix::date::Time::new(timestamp, 0),
    };
    let commit = Commit {
        tree: tree_oid,
        parents: parents.into(),
        author: sig.clone(),
        committer: sig,
        encoding: None,
        message: title.into(),
        extra_headers: Vec::new(),
    };
    repo.write_object(&commit).expect("write commit").detach()
}

/// Updates `ref_name` in `repo` to point at `target`.
pub fn set_ref(repo: &gix::Repository, ref_name: &str, target: gix::ObjectId) {
    repo.edit_reference(RefEdit {
        change: Change::Update {
            log: LogChange {
                mode: RefLog::AndReference,
                force_create_reflog: false,
                message: "test fixture".into(),
            },
            expected: PreviousValue::Any,
            new: gix::refs::Target::Object(target),
        },
        name: ref_name.try_into().expect("valid ref name"),
        deref: false,
    })
    .expect("update ref");
}

fn append_git_user_config(config_path: &Path) {
    let mut existing = std::fs::read_to_string(config_path).unwrap_or_default();
    existing.push_str("\n[user]\n\tname = LGTM Reviewer\n\temail = reviewer@lgtm.local\n");
    std::fs::write(config_path, existing).expect("write git user config");
}

fn write_worktree_stage(workspace_root: &Path, files: &[(&str, &str)]) {
    for (rel_path, content) in files {
        let target = workspace_root.join(rel_path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).expect("create parent dirs");
        }
        std::fs::write(&target, content).expect("write worktree file");
    }
}

fn write_subtree(
    repo: &gix::Repository,
    entries: &BTreeMap<String, gix::ObjectId>,
    prefix: &str,
) -> gix::ObjectId {
    let mut direct_files: BTreeMap<String, gix::ObjectId> = BTreeMap::new();
    let mut subdirs: BTreeSet<String> = BTreeSet::new();

    for (path, oid) in entries {
        let rel = if prefix.is_empty() {
            path.as_str()
        } else if let Some(stripped) = path.strip_prefix(prefix).and_then(|s| s.strip_prefix('/')) {
            stripped
        } else {
            continue;
        };
        if let Some((dir, _)) = rel.split_once('/') {
            subdirs.insert(dir.to_owned());
        } else {
            direct_files.insert(rel.to_owned(), *oid);
        }
    }

    let mut tree_entries: Vec<Entry> = Vec::new();
    for (filename, oid) in direct_files {
        tree_entries.push(Entry {
            mode: EntryKind::Blob.into(),
            filename: filename.into(),
            oid,
        });
    }
    for dir in subdirs {
        let sub_prefix = if prefix.is_empty() {
            dir.clone()
        } else {
            format!("{prefix}/{dir}")
        };
        let sub_oid = write_subtree(repo, entries, &sub_prefix);
        tree_entries.push(Entry {
            mode: EntryKind::Tree.into(),
            filename: dir.into(),
            oid: sub_oid,
        });
    }

    tree_entries.sort_by(|a, b| {
        let a_key = if a.mode.is_tree() {
            format!("{}/", a.filename)
        } else {
            a.filename.to_string()
        };
        let b_key = if b.mode.is_tree() {
            format!("{}/", b.filename)
        } else {
            b.filename.to_string()
        };
        a_key.cmp(&b_key)
    });

    let tree = Tree {
        entries: tree_entries,
    };
    repo.write_object(&tree).expect("write tree").detach()
}
