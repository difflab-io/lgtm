//! `GitRepoFixture` — a tiny git repo in a temp dir with one base commit and
//! deliberate working-tree modifications so tests can exercise diffs without
//! depending on OSS fixtures.
//!
//! The standard fixture:
//! - Base commit touches `src/lib.rs`, `src/util.rs`, `Cargo.toml`.
//! - Working tree modifies `src/lib.rs`, adds `src/added.rs`, deletes `src/util.rs`.
//!
//! `base_ref = "HEAD"` compares the working tree against the base commit —
//! so a test can assert the expected set of changes without knowing SHAs.

use std::fs;
use std::path::{Path, PathBuf};

use git2::{Repository, Signature};
use tempfile::TempDir;

pub struct GitRepoFixture {
    _tmp: TempDir,
    path: PathBuf,
}

impl GitRepoFixture {
    pub fn standard() -> Self {
        let tmp = tempfile::tempdir().expect("tempdir for repo");
        // Canonicalize so the fixture path matches what consumers see after
        // their own canonicalize() (e.g. `lgtm-cli` does this). On macOS the
        // raw tempdir is `/var/folders/...` while canonical is `/private/var/...`.
        let path = std::fs::canonicalize(tmp.path()).expect("canonicalize tempdir");
        init_standard_repo(&path);
        Self { _tmp: tmp, path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn init_standard_repo(path: &Path) {
    let repo = Repository::init(path).expect("git init");

    fs::create_dir_all(path.join("src")).unwrap();
    fs::write(path.join("Cargo.toml"), "[package]\nname = \"fixture\"\n").unwrap();
    fs::write(path.join("src/lib.rs"), "pub fn hello() {}\n").unwrap();
    fs::write(path.join("src/util.rs"), "pub fn util() {}\n").unwrap();

    let sig = Signature::now("fixture", "fixture@example.com").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("Cargo.toml")).unwrap();
    index.add_path(Path::new("src/lib.rs")).unwrap();
    index.add_path(Path::new("src/util.rs")).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "base", &tree, &[])
        .unwrap();

    // Working-tree changes vs. that base commit:
    //   modified: src/lib.rs
    //   deleted:  src/util.rs
    //   added:    src/added.rs
    fs::write(
        path.join("src/lib.rs"),
        "pub fn hello() {}\npub fn hi() {}\n",
    )
    .unwrap();
    fs::remove_file(path.join("src/util.rs")).unwrap();
    fs::write(path.join("src/added.rs"), "pub const X: u32 = 1;\n").unwrap();
}
